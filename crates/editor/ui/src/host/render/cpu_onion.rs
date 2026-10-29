//! CPU 洋葱皮接入 Host 渲染链路（Q3 B：GPU 只保留当前轨 + CPU 叠加层）。
//!
//! # 分工
//!
//! - **GPU**：当前轨（主音轨）——常驻 GPU 音符缓冲，事件级增量更新；
//! - **CPU**（本模块）：非当前轨 + 非静音轨——`lumino-cpu-onion` 光栅成透明背景
//!   RGBA8 帧，经 `lumino_gfx::CpuOnionFrame` 交渲染线程作为叠加层绘制
//!   （网格之后、GPU 洋葱皮与当前轨之前）。
//!
//! # 每帧流程（`collect_render_data` → 本模块 → `RenderData` → `RenderParams`）
//!
//! 1. 后端不是 `Cpu` → 彻底停用（释放渲染器/索引），`frame = None`（默认路径零变化）；
//! 2. 走带模式 / 纵向卷帘 / 无文档 / 相机退化 → 本帧不绘制；
//! 3. 相机换算到**物理像素**（与 GPU 主轨屏幕坐标同口径，见 [`camera`]）；
//! 4. 静音表按**文档轨数**展开（侧栏音轨 id 越界安全忽略）；
//! 5. 状态机冻结：指纹不变 → 复用上一帧与同一 `version`；变化 → 重新 `render_full`、
//!    版本递增并置内容脏（`mark_render_content_dirty`），保证 present 等到新帧。
//!
//! # 本步已知边界（Step 5 / Step 4b 接管）
//!
//! - `track_notes_gen` 变化时**整份索引重建**（未做增量/脏区）；
//! - 每帧像素**整帧拷贝**（`Vec<u8>` → `Arc<[u8]>`，未做局部上传）；
//! - 纵向卷帘本步直接跳过 CPU 帧（坐标未做转置，画上去会错位）；
//!   走带模式的音符层复用 GPU 段表绘制全部音轨，因此走带布局下不启用单轨模式；
//! - Step 4a 只在**切换点**设闸：加载 MIDI 后禁止切换（`onion_backend_locked`）、
//!   走带/瀑布流/导出中/视频导出面板仅打开时拒绝切到 CPU、纵向卷帘下切 CPU 会先自动
//!   退回横向（`host::dialog::apply::onion`）；
//! - Step 4b 补**入口**闸：已处在 CPU 后端时，纵向卷帘 / 工程走带 / 全屏瀑布流 /
//!   右侧栏钢琴瀑布预览 / 视频导出面板与开始导出一律拒绝 + Toast
//!   （`root::handlers::cpu_guard`，Runner 侧另有最终兜底），因此下面的
//!   「纵向往带直接跳过」分支只是**纵深防御**，不再是用户可达的静默降级。

use lumino_core::storage::config::OnionSkinBackend;
use lumino_gfx::CpuOnionFrame;
use lumino_midi_model::MidiDocument;

use crate::host::Host;
use crate::host::render::onion_skin::mute_fingerprint_of;
use crate::sidebar::Track;

mod camera;
mod plan;
mod state;

pub(crate) use state::CpuOnionState;

use camera::CpuOnionCameraInput;
use state::{CpuOnionFrameKey, CpuOnionOutput};

impl Host {
    /// 当前洋葱皮渲染后端（只读）。
    pub fn onion_skin_backend(&self) -> OnionSkinBackend {
        self.onion_skin_backend
    }

    /// 洋葱皮后端是否锁定（**加载 MIDI 后禁止切换**的用户硬约束）。
    ///
    /// 判定取会话锁（`host::session_lock::MidiLoadLock`），即「**是否加载过真实
    /// MIDI/工程**」这一单一真源：
    /// - 真实 MIDI/工程载入 → `Host::set_midi_document` → 锁定；
    /// - 启动 / 新建 / 关闭工程 → `Host::init_blank_project` → 解锁。
    ///
    /// **刻意不用 `document.is_some()`**：启动即会 `init_blank_project` 挂一份 2 轨
    /// 空白 `MidiDocument`，用「有文档」当锁会让设置页后端下拉从第一帧起永久锁定——
    /// 空白工程里既选不了 CPU 也切不回 GPU，功能等于不可用。用户约束是「加载 MIDI
    /// 后禁止切换」，空白工程不是加载 MIDI。
    ///
    /// 设置对话框是独立 `Host`（Root 里没有会话状态），因此锁定态必须由主窗口
    /// 主动注入（`Host::set_settings_onion_backend_locked`），对话框不得自行判定。
    pub fn onion_backend_locked(&self) -> bool {
        self.midi_load.is_loaded()
    }

    /// 切换洋葱皮渲染后端（由设置页触发；调用前必须过 `apply_onion_backend_switch` 守卫）。
    ///
    /// 切换必须让三样缓存都失效：
    /// - GPU 段表：单轨 ↔ 全轨语义变化 → 强制下一次全量会话；
    /// - CPU 帧：指纹包含后端语义，直接清空由下一帧重建；
    /// - 编辑器侧的 CPU 洋葱皮脏区记录开关：只有 CPU 后端才值得为每笔编辑付费
    ///   （见 `EditorData::set_cpu_onion_dirty_enabled`，默认 GPU 零开销）。
    pub fn set_onion_skin_backend(&mut self, backend: OnionSkinBackend) {
        if self.onion_skin_backend == backend {
            return;
        }
        self.onion_skin_backend = backend;
        self.render_ctx.onion_skin_state.force_full_next();
        self.cpu_onion_state.deactivate();
        // 切到 CPU 开启记录；切回 GPU 关闭并清空残留脏区（关闭动作内含 clear）
        self.root
            .editor
            .editor_state
            .data
            .set_cpu_onion_dirty_enabled(backend == OnionSkinBackend::Cpu);
        self.mark_dirty();
    }

    /// 生成/复用 CPU 洋葱皮帧（每帧由 `collect_render_data` 调用）。
    ///
    /// 返回 `Some(frame)` 时调用方必须把它放进 `RenderData::cpu_onion_frame`，
    /// 由 `build_render_params` 透传到 `RenderParams::cpu_onion_frame`。
    ///
    /// 只有**重新光栅化**的帧才置内容脏（`render_content_dirty`）——复用帧与
    /// 「只更新快照/索引」的帧都不置位，否则纯滚动/当前轨拖动会退化成每帧等待
    /// 渲染线程，present 节奏崩掉。
    pub(crate) fn update_cpu_onion_frame(&mut self) -> Option<CpuOnionFrame> {
        // 默认后端是 GPU：本步完全不参与，零行为变化
        if self.onion_skin_backend != OnionSkinBackend::Cpu {
            self.cpu_onion_state.deactivate();
            // 防御：异常路径下残留的脏区不得泄漏到下一次切回 CPU
            self.root.editor.editor_state.data.clear_cpu_onion_dirty();
            return None;
        }
        let output = self.build_cpu_onion_output();
        if output.rebuilt {
            self.render_ctx.mark_render_content_dirty();
        }
        output.frame
    }

    /// 收集相机/静音/指纹/脏区并驱动帧状态机（不置内容脏，由调用方决定）。
    fn build_cpu_onion_output(&mut self) -> CpuOnionOutput {
        // 走带模式：渲染通道在 arrangement 分支提前返回，CPU 叠加层结构上不会绘制；
        // 纵向卷帘：本步坐标未转置（Step 4 禁用 CPU 后端），跳过以免错位。
        //
        // **不 take 脏区**：这两条路径只是「暂时看不到」，回到钢琴卷帘后必须一次性
        // 消费掉这段时间的编辑，否则脏区被静默丢弃 → CPU 帧永久停在旧内容。
        if self.root.is_arrangement_mode() || self.root.editor.editor_state.is_vertical_roll {
            self.cpu_onion_state.clear_frame();
            return CpuOnionOutput {
                frame: None,
                rebuilt: false,
            };
        }

        let input = {
            let es = &self.root.editor.editor_state;
            CpuOnionCameraInput {
                scale_factor: self.render_ctx.viewport.scale_factor(),
                canvas_offset: (es.canvas.offset_x, es.canvas.offset_y),
                canvas_size: (es.canvas.size_x, es.canvas.size_y),
                // 与 GPU 主轨同一数据源：钢琴卷帘的 scroll/zoom（逻辑像素）
                scroll: (es.view.scroll_x, es.view.scroll_y),
                zoom: (es.view.zoom_x, es.view.zoom_y),
                key_count: es.view.visible_key_count,
            }
        };
        let track_count = self
            .root
            .editor
            .editor_state
            .data
            .document
            .as_ref()
            .map_or(0, MidiDocument::track_count);
        let Some(geometry) = input.to_physical() else {
            self.cpu_onion_state.clear_frame();
            return CpuOnionOutput {
                frame: None,
                rebuilt: false,
            };
        };
        if track_count == 0 {
            // 无文档：本帧不绘制；脏区保留（下次加载必然走整帧兜底）
            self.cpu_onion_state.clear_frame();
            return CpuOnionOutput {
                frame: None,
                rebuilt: false,
            };
        }

        let (muted, mut muted_ids) = collect_muted_tracks(&self.root.sidebar.tracks, track_count);
        // 每帧消费一次脏区；`onion_dirty_tracks` 是「变化来源」的独立真源
        // （GPU 洋葱皮也在用，不能 take，只 clone）。
        let (dirty, dirty_tracks) = {
            let data = &mut self.root.editor.editor_state.data;
            (data.take_cpu_onion_dirty(), data.onion_dirty_tracks.clone())
        };
        let es = &self.root.editor.editor_state;
        let Some(document) = es.data.document.as_ref() else {
            self.cpu_onion_state.clear_frame();
            return CpuOnionOutput {
                frame: None,
                rebuilt: false,
            };
        };
        let key = CpuOnionFrameKey {
            track_notes_gen: es.data.track_notes_gen,
            current_track: es.data.current_track,
            mute_fingerprint: mute_fingerprint_of(&mut muted_ids),
            palette_idx: lumino_extras::palette::current_palette_idx(),
            geometry,
        };
        self.cpu_onion_state
            .update(key, document, &muted, dirty, dirty_tracks.as_ref())
    }
}

/// 收集静音映射：`(文档轨长的静音表, 静音音轨 id 列表)`。
///
/// 侧栏音轨 id 可能超出文档轨数（删轨/协作同步中间态）——越界 id 只进指纹、
/// 不进静音表，绝不越界索引。
fn collect_muted_tracks(tracks: &[Track], track_count: usize) -> (Vec<bool>, Vec<usize>) {
    let mut muted = vec![false; track_count];
    let mut ids = Vec::new();
    for track in tracks.iter().filter(|track| track.is_muted) {
        if let Some(slot) = muted.get_mut(track.id) {
            *slot = true;
        }
        ids.push(track.id);
    }
    (muted, ids)
}

/// CPU 洋葱皮帧生成测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "cpu_onion/tests.rs"]
mod tests;
/// CPU 洋葱皮脏区/增量索引/当前轨跳过测试（Step 5a）
#[cfg(test)]
#[path = "cpu_onion/tests_dirty.rs"]
mod tests_dirty;
