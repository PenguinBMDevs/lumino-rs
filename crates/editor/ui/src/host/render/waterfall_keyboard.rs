//! 钢琴瀑布流键盘离屏渲染的按需入口拆分
//!
//! 从 `render.rs::ensure_piano_waterfall_keyboard` 抽出：参数快照、三条渲染路径、
//! 脏签名折叠与渲染器懒创建。三条路径的调用顺序、状态写入顺序与 GPU 上传时机
//! 均与原实现逐句一致。

use std::sync::Arc;

use lumino_gfx::GpuEpoch;
use wgpu;

use crate::host::Host;
use crate::right_sidebar::core::RESIZE_HANDLE_WIDTH;
use crate::right_sidebar::piano_waterfall::keyboard_renderer::{
    KEY_HEIGHT_RATIO, KeyboardRenderer, MAX_KEY_HEIGHT, MIN_KEY_HEIGHT,
};

/// 一次 `ensure` 调用的参数快照。
///
/// `take_note_data` 只采集一次，三条路径共用同一份活体缓冲句柄——避免重复取数
/// 破坏「音符数据禁止第二份拷贝」的约束。
pub(super) struct WaterfallParams {
    epoch: GpuEpoch,
    pub(super) in_waterfall: bool,
    key_count: u32,
    zoom_x: f32,
    scroll_x: f32,
    current_track: u32,
    note_data: Option<(wgpu::Buffer, u32)>,
    note_count: u32,
}

/// 离屏渲染的一次请求：把 `render_scene` 的多个参数收进结构体，
/// 让路径辅助函数保持在 `too_many_arguments` 阈值内。
struct SceneArgs {
    width: u32,
    height: u32,
    key_count: u32,
    note_data: Option<(wgpu::Buffer, u32)>,
    zoom_x: f32,
    scroll_x: f32,
    current_track: u32,
}

/// 脏判断签名折叠：逐字段 `wrapping_mul(31).wrapping_add`，
/// 与原先手写链完全等价（首字段相当于从 0 折叠而来）。
fn fold_signature(fields: &[u64]) -> u64 {
    let mut sig = 0u64;
    for &field in fields {
        sig = sig.wrapping_mul(31).wrapping_add(field);
    }
    sig
}

impl Host {
    /// 采集参数并完成跨代次缓存判废（必须在任何渲染动作之前）。
    pub(super) fn collect_waterfall_params(&mut self) -> WaterfallParams {
        use crate::titlebar::mode_toggle::AppMode;

        let epoch = self.render_ctx.epoch;

        // REND-019 PR-B：跨代次判废面板缓存视图（epoch 恒同代时无行为变化；
        // PR-C 重连后旧视图整体丢弃，走既有重渲染路径）
        {
            let rs = &mut self.root.right_sidebar.piano_waterfall;
            if rs.cached_epoch.is_some_and(|e| e != epoch) {
                rs.waterfall_view = None;
                rs.cached_signature = None;
                rs.cached_epoch = None;
            }
            let fp = &mut self.root.waterfall_player;
            if fp.cached_epoch.is_some_and(|e| e != epoch) {
                fp.view = None;
                fp.cached_signature = None;
                fp.cached_epoch = None;
            }
        }

        let in_waterfall = self.root.state.current_mode == AppMode::Waterfall;

        // 键数跟随全局设置：开启 256 键扩展则为 256，否则 128
        let key_count: u32 = if self.root.settings.display.enable_256key {
            256
        } else {
            128
        };

        // 视口参数（与钢琴卷帘一致，驱动音符落点 / 时间流 / 主音轨蓝）
        let zoom_x = self.root.editor.editor_state.view.zoom_x;
        let scroll_x = self.root.editor.editor_state.view.scroll_x;
        let current_track = self.root.editor.editor_state.data.current_track as u32 + 1;

        // 复用渲染线程发布的活体 GPU 音符实例缓冲（零拷贝）
        let note_data = self
            .render_ctx
            .wgpu_render_thread
            .as_ref()
            .and_then(|t| t.take_note_data());
        let note_count = note_data.as_ref().map(|(_, c)| *c).unwrap_or(0);

        if std::env::var("LUMINO_VRDIAG").is_ok() {
            tracing::info!(
                "[VR_DIAG] ensure: in_waterfall={} size={:?} key_count={} note_count={}",
                in_waterfall,
                *self.root.waterfall_player.size.borrow(),
                key_count,
                note_count
            );
        }

        WaterfallParams {
            epoch,
            in_waterfall,
            key_count,
            zoom_x,
            scroll_x,
            current_track,
            note_data,
            note_count,
        }
    }

    /// 跨代次判废渲染器 + 懒创建 + 离屏渲染（GPU 上传在此完成，是最后一次写操作）。
    fn draw_scene(&mut self, epoch: GpuEpoch, args: SceneArgs) -> Option<Arc<wgpu::TextureView>> {
        // REND-019 PR-B：跨代次判废键盘渲染器（恒同代时无行为变化；
        // 失配丢弃后走既有懒创建路径）
        if self
            .render_ctx
            .keyboard_renderer
            .as_ref()
            .is_some_and(|r| r.gpu_epoch() != epoch)
        {
            self.render_ctx.keyboard_renderer = None;
        }
        let renderer = self
            .render_ctx
            .keyboard_renderer
            .get_or_insert_with(|| KeyboardRenderer::new(&self.render_ctx.device, epoch));

        // 同步离屏渲染：返回 Some 即拿到本帧纹理视图（无 CPU 读回、无异步）；
        // 上一代 `shader` 图元会在自身渲染通道内直接采样该视图合成（GPU→GPU，不闪烁）。
        renderer.render_scene(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            args.width,
            args.height,
            args.key_count,
            args.note_data,
            args.zoom_x,
            args.scroll_x,
            args.current_track,
        )
    }

    /// 全屏 Waterfall 模式。返回 `true` 表示后续路径应停止（与原实现的 return 对应）。
    pub(super) fn render_fullscreen_waterfall(&mut self, params: &WaterfallParams) -> bool {
        use crate::titlebar::mode_toggle::AppMode;

        // ── 瀑布流渲染：全屏 Waterfall 模式 ——
        // 播放器恒为「固定下落」模式：底部键盘线（落点线）钉在当前播放位置，
        // 音符等速连续下落到键盘。**刻意不复用卷帘的 `scroll_x`**——卷帘处于
        // 「自动翻页」时 `scroll_x` 只在播放头触边时整屏跳变（播放器表现为
        // 「停住 → 跳页」），「关闭」自动滚动时更是完全静止。
        // 与卷帘的自动滚动模式解耦见 `Editor::waterfall_player_scroll_x`。
        let player_scroll_x = self
            .root
            .editor
            .waterfall_player_scroll_x(self.root.editor.playback_position);

        let size = *self.root.waterfall_player.size.borrow();
        // 尚无布局尺寸时不再早退：用兜底尺寸先把纹理建出来（键盘立即可见），
        // responsive 写回真实尺寸后签名变化会触发下一帧重渲染到精确尺寸，
        // 打破「尺寸 None → 早退 → 永不渲染」的死循环。
        let (width, height) = size.unwrap_or((1280, 720));

        // 脏判断签名：任意参数变化即重绘（滚动/缩放变化 → 瀑布流实时跟随）
        let sig = fold_signature(&[
            width as u64,
            height as u64,
            params.key_count as u64,
            params.zoom_x as i64 as u64,
            player_scroll_x as i64 as u64,
            params.current_track as u64,
            params.note_count as u64,
        ]);

        {
            let state = &self.root.waterfall_player;
            if state.cached_signature == Some(sig) {
                return true; // 参数未变，复用已渲染纹理视图
            }
        }

        // REND-019 PR-B：跨代次判废键盘渲染器（恒同代时无行为变化；
        // 失配丢弃后走既有懒创建路径）
        let diag = std::env::var("LUMINO_VRDIAG").is_ok();
        let scene = self.draw_scene(
            params.epoch,
            SceneArgs {
                width,
                height,
                key_count: params.key_count,
                note_data: params.note_data.clone(),
                zoom_x: params.zoom_x,
                scroll_x: player_scroll_x,
                current_track: params.current_track,
            },
        );
        {
            let state = &mut self.root.waterfall_player;
            if let Some(view) = scene {
                state.view = Some(view);
                state.cached_signature = Some(sig);
                state.cached_epoch = Some(params.epoch);
                if diag {
                    tracing::info!(
                        "[VR_DIAG] render_scene OK w={width} h={height} note_count={}",
                        params.note_count
                    );
                }
            } else if diag {
                tracing::info!(
                    "[VR_DIAG] render_scene NONE w={width} h={height} note_count={}",
                    params.note_count
                );
            }
        }

        // 瀑布流全屏模式下右侧栏预览被隐藏：释放其纹理与签名，停止一切渲染动作
        let rs = &mut self.root.right_sidebar.piano_waterfall;
        if rs.waterfall_view.is_some() {
            rs.waterfall_view = None;
            rs.cached_signature = None;
            rs.cached_epoch = None;
        }
        // 原实现此处以 `current_mode == Waterfall` 早退；由调用方据此终止后续路径
        self.root.state.current_mode == AppMode::Waterfall
    }

    /// 视频剪辑窗口预览：以播放头处「素材源帧」为锚点。
    pub(super) fn render_renderer_entry_preview(&mut self, params: &WaterfallParams) {
        // ── 渲染器入口（视频剪辑窗口）瀑布流预览 ──
        // 当 Renderer 分组首级面板激活（且未进入 Audio/Video 子面板）时，在 waterfall_player 上离屏渲染
        // 对标 PianoRoll 的 File/Automation 子面板逻辑，复用全屏 waterfall 的渲染路径但由剪辑窗口的 responsive 驱动尺寸
        //
        // 判定统一走 `Root::is_renderer_entry_active()`（唯一权威源）：此前此处
        // 内联复制了一份同款条件，任何一处补条件（如工程走带优先级）都会漏改，
        // 渲染与交互各自为政。
        let size = *self.root.waterfall_player.size.borrow();
        let (width, height) = size.unwrap_or((1920, 1080));
        // 画面进出显示：剪辑面板预览以「播放头处的素材源帧」为渲染锚点。
        // 瀑布流 shader 的落点线 tick = scroll_x / zoom_x，故把锚点 tick 回写
        // scroll_x（zoom_x 保留卷帘尺度）——播放头或出入点一变，画面即跟随。
        //
        // `clip_preview_tick()` 返回 None 表示播放头不在素材带上：此时必须
        // **释放预览纹理**，让画面回落到「无素材带」占位，绝不显示瀑布流。
        // 两种状态（在带 / 不在带）都进签名，状态切换即触发一次刷新。
        let anchor_tick = self.root.clip_preview_tick();
        let anchor_scroll_x = anchor_tick.unwrap_or(0) as f32 * params.zoom_x;
        let sig = fold_signature(&[
            width as u64,
            height as u64,
            params.key_count as u64,
            params.zoom_x as i64 as u64,
            anchor_scroll_x as i64 as u64,
            params.current_track as u64,
            params.note_count as u64,
            u64::from(anchor_tick.is_some()),
        ]);

        let unchanged = self.root.waterfall_player.cached_signature == Some(sig);
        if !unchanged {
            match anchor_tick {
                Some(_) => {
                    // REND-019 PR-B：跨代次判废（同上，恒同代时无行为变化）
                    let scene = self.draw_scene(
                        params.epoch,
                        SceneArgs {
                            width,
                            height,
                            key_count: params.key_count,
                            note_data: params.note_data.clone(),
                            zoom_x: params.zoom_x,
                            scroll_x: anchor_scroll_x,
                            current_track: params.current_track,
                        },
                    );
                    let state = &mut self.root.waterfall_player;
                    if let Some(view) = scene {
                        state.view = Some(view);
                        state.cached_signature = Some(sig);
                        state.cached_epoch = Some(params.epoch);
                    }
                }
                None => {
                    // 播放头在素材带之外：无帧可显示，释放纹理（画面占位由 UI 呈现）
                    let state = &mut self.root.waterfall_player;
                    state.view = None;
                    state.cached_signature = Some(sig);
                    state.cached_epoch = Some(params.epoch);
                }
            }
        }

        // 剪辑窗口接管瀑布流预览，右侧栏预览应清空避免双重渲染
        let rs = &mut self.root.right_sidebar.piano_waterfall;
        if rs.waterfall_view.is_some() {
            rs.waterfall_view = None;
            rs.cached_signature = None;
            rs.cached_epoch = None;
        }
    }

    /// 右侧栏瀑布流预览。参数快照按值传入，最后一条路径直接移动 `note_data`。
    pub(super) fn render_sidebar_preview(&mut self, params: WaterfallParams) {
        // ── 右侧栏瀑布流预览 ──
        // 仅当右侧栏确实可见（且为瀑布流面板）时才渲染：关闭面板 / 切换面板 /
        // 进入瀑布流全屏模式 / 走带 / 导出面板 等任一情形均彻底停渲。
        let active = self.root.right_sidebar.panel_visible
            && self.root.right_sidebar.active_panel
                == crate::right_sidebar::RightSidebarPanel::PianoWaterfall
            && self.root.right_sidebar_visible();

        if !active {
            let state = &mut self.root.right_sidebar.piano_waterfall;
            // 面板不可见：释放已渲染纹理视图与签名，避免陈旧显示与显存占用
            if state.waterfall_view.is_some() {
                state.waterfall_view = None;
                state.cached_signature = None;
                state.cached_epoch = None;
            }
            // 同步清空全屏播放器缓存（从瀑布流模式切回时）
            let fp = &mut self.root.waterfall_player;
            if fp.view.is_some() {
                fp.view = None;
                fp.cached_signature = None;
                fp.cached_epoch = None;
            }
            return;
        }

        // 键盘实际绘制宽度 = 面板内容宽（与显示宽度一致，无边框/留白，保证清晰不拉伸）
        let width = (self.root.right_sidebar.panel_width - RESIZE_HANDLE_WIDTH).max(1.0) as u32;
        // 面板内容高 = 窗口逻辑高（瀑布流占满面板纵轴，键盘贴底）
        let panel_height = self.render_ctx.viewport.logical_size().height.max(1.0);
        let kb_h = (width as f32 * KEY_HEIGHT_RATIO).clamp(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
        let height = (panel_height).max(kb_h + 1.0) as u32;

        // 脏判断签名：任意参数变化即重绘（滚动/缩放变化 → 瀑布流实时跟随）
        let sig = fold_signature(&[
            width as u64,
            height as u64,
            params.key_count as u64,
            params.zoom_x as i64 as u64,
            params.scroll_x as i64 as u64,
            params.current_track as u64,
            params.note_count as u64,
        ]);

        {
            let state = &self.root.right_sidebar.piano_waterfall;
            if state.cached_signature == Some(sig) {
                return; // 参数未变，复用已渲染纹理视图
            }
        }

        // REND-019 PR-B：跨代次判废（同上，恒同代时无行为变化）
        let scene = self.draw_scene(
            params.epoch,
            SceneArgs {
                width,
                height,
                key_count: params.key_count,
                note_data: params.note_data,
                zoom_x: params.zoom_x,
                scroll_x: params.scroll_x,
                current_track: params.current_track,
            },
        );
        if let Some(view) = scene {
            let state = &mut self.root.right_sidebar.piano_waterfall;
            state.waterfall_view = Some(view);
            state.cached_signature = Some(sig);
            state.cached_epoch = Some(params.epoch);
        }
    }
}
