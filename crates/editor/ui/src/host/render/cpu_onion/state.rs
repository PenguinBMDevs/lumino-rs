//! CPU 洋葱皮帧状态（Step 5a：脏区重绘 + 增量索引 + 当前轨编辑跳过）。
//!
//! # 与 Step 3（整帧冻结）的差异
//!
//! Step 3 的语义是「任何输入变化 → 整份索引 + 整帧重画」。Step 5a 把它拆成三层，
//! 由纯函数 [`plan_update`] 判定，本模块只负责执行：
//!
//! | 场景 | 索引 | 光栅 |
//! |------|------|------|
//! | 相机/静音/调色板/窗口尺寸变化 | 不动 | 整帧 |
//! | 洋葱轨单笔插入/删除/更新 | 增量（`with_changes_fast`） | 该音符的像素脏区 |
//! | 批量/整轨替换/区间删除 | 整份重建 | 整帧 |
//! | **当前轨**编辑（CPU 不画它） | 增量或延迟补齐 | **不重绘、不递增版本** |
//! | 当前轨切换 | 补齐旧当前轨几何 | 整帧 |
//! | 代次变了但没有脏区来源 | 整份重建 | 整帧 |
//!
//! # 脏区透传（Step 5b）
//!
//! 区域重绘时把实际矩形同时写进两处：`CpuOnionOutput::dirty`（给调用方/测试）与
//! `CpuOnionFrame::dirty_rect`（给 gfx，局部上传用）。整帧重绘与复用帧一律
//! `None`——复用帧的 `version` 没变，旧脏区必须清掉，否则下游可能把它误当本帧
//! 增量范围。版本只增不减（见 [`CpuOnionState::deactivate`]），否则 gfx 侧按
//! `(size, version)` 判重会把新会话首帧误判成 `Reuse`。
//!
//! # 直接 `track_notes_mut` 编辑的几何补齐
//!
//! 拖动 / 批量编辑 / 连奏直接改 `track_notes_mut`（不经过插入/删除/更新 API），
//! 因此不会有按值脏区，只能靠 `onion_dirty_tracks` 的受影响轨集合识别。处理分两类：
//!
//! - **非当前轨**（含静音轨）：当场 `rebuild_track_geometry` 补齐。静音轨虽然本帧
//!   不画，但取消静音只是布局变化（只重绘、不重建索引）——几何过期会**静默漏画
//!   长音符**；协作远端 / 未来直写路径完全可能直接改「非当前且静音」的轨。
//! - **当前轨**：CPU 帧不画它，保留在 `stale_tracks` 里，等它不再是当前轨
//!   （切轨，含同一帧内切轨）时再补——热路径不为它付 O(该轨音符数)。
//!
//! # 版本与内容脏
//!
//! 只有**真正重新光栅化**才递增 `version` 并让调用方置内容脏；只更新快照/索引
//! 的帧不递增版本（否则派生渲染侧的判重失效，present 每帧等待）。

use std::collections::HashSet;
use std::sync::Arc;

use lumino_cpu_onion::{CpuOnionRenderer, TrackIndex};
use lumino_editor_state::CpuOnionDirty;
use lumino_gfx::CpuOnionFrame;
use lumino_midi_model::MidiDocument;

use super::camera::CpuOnionGeometry;
use super::density_cache::DensityCache;
use super::output::{CpuOnionOutput, to_gfx_rect};
use super::plan::{CpuOnionIndex, CpuOnionRedraw, PlanInput, overview_redraw, plan_update};

mod bands;
mod maintain;
mod scroll;

use bands::worker_bands;
/// 帧冻结点：这些输入任一变化都必须重新光栅化。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CpuOnionFrameKey {
    /// 文档音符代次（`EditorData::track_notes_gen`）
    pub track_notes_gen: u64,
    /// 当前音轨（CPU 帧跳过它，由 GPU 主轨绘制）
    pub current_track: usize,
    /// 静音集合指纹（顺序无关）
    pub mute_fingerprint: u64,
    /// 当前调色板索引（实例颜色固化在像素里）
    pub palette_idx: u8,
    /// 物理矩形 + 相机（含尺寸、滚动、缩放、键数）
    pub geometry: CpuOnionGeometry,
}

impl CpuOnionFrameKey {
    /// 布局子键是否一致（**不含** `track_notes_gen`：代次变化不一定改布局）。
    fn layout_eq(&self, other: &Self) -> bool {
        self.current_track == other.current_track
            && self.mute_fingerprint == other.mute_fingerprint
            && self.palette_idx == other.palette_idx
            && self.geometry == other.geometry
    }
}

/// CPU 洋葱皮运行时状态（渲染器 + 文档快照 + 增量索引 + 帧冻结）。
#[derive(Debug, Default)]
pub(crate) struct CpuOnionState {
    /// 渲染器（首次使用时惰性创建，内部持有 tokio 多线程 runtime）
    renderer: Option<CpuOnionRenderer>,
    /// 文档快照（块级浅拷贝，不复制音符大块）
    document: Option<Arc<MidiDocument>>,
    /// 派生索引（对应 `index_gen` 代次的文档快照）
    index: Option<Arc<TrackIndex>>,
    /// 当前帧（`None` = 不绘制）
    frame: Option<CpuOnionFrame>,
    /// 帧冻结点（`None` = 无有效帧）
    frozen: Option<CpuOnionFrameKey>,
    /// 帧内容版本（每次重渲染递增；渲染侧据此判重上传）
    version: u64,
    /// 索引/快照对应的 `track_notes_gen`
    index_gen: Option<u64>,
    /// 渲染器调色板缓存对应的调色板索引
    palette_idx: Option<u8>,
    /// 索引几何可能过期的音轨集合（直接 `track_notes_mut` 编辑跳过索引更新）。
    ///
    /// 登记时机：代次变了但没有任何原子变更记录（`dirty == None` 或 changes 为空）
    /// ——说明来源是直接写路径，索引没法用 changes 增量表达。此时把
    /// `dirty_tracks` 里的**每一条**受影响轨都登记进来：
    /// - 非当前轨（含**静音**轨）：本帧或下一帧按最新文档 `rebuild_track_geometry`
    ///   当场补齐——协作远端 / 未来直写路径可能直接改「非当前且静音」的轨，
    ///   取消静音后它就是必须画的洋葱轨，几何过期会**静默漏画长音符**；
    /// - 当前轨：CPU 本帧不画它，登记后保留到切轨那一刻再补（热路径不回退）。
    stale_tracks: HashSet<usize>,
    /// 总览密度 LOD 缓存（按代次/键数失效；单笔编辑增量维护）。
    density: DensityCache,
    /// 上一帧是否处于任一简化层（彩色覆盖或 >=30 小节热力图兜底）。
    ///
    /// 跨档必须整帧：退出简化层时哪怕本帧只是「本来不重绘」的当前轨编辑，也必须
    /// 换回 Exact 音符；简化层不能留下过期像素。
    simplified_active: bool,
}

impl CpuOnionState {
    /// 本帧不绘制，但保留缓存（相机退化 / 纵向卷帘 / 无文档）。
    pub(crate) fn clear_frame(&mut self) {
        self.frame = None;
        self.frozen = None;
    }

    /// 彻底停用（后端 = GPU）：释放渲染器、文档快照与索引。
    ///
    /// 默认后端就是 GPU，且 CPU 模式可能长期不被使用——不能为了「切回更快」
    /// 而长期驻留一份索引/渲染缓冲。切回 CPU 时重建（一次 O(总音符数)）。
    ///
    /// **版本单调性必须保留**：`version` 是 gfx 侧判断「纹理内容是否同一份」的
    /// 唯一标识。若这里把它归零，重新启用后的首帧版本会重新从 1 开始，而同尺寸、
    /// 同版本的旧纹理仍驻留在渲染线程 → 被误判为 `Reuse`，画面上还是上一会话
    /// 的旧图。空白工程里「CPU → GPU →（编辑）→ CPU」完全可达（`MIDI` 加载后
    /// 才禁止切换），因此版本只增不减。
    pub(crate) fn deactivate(&mut self) {
        if self.renderer.is_none()
            && self.document.is_none()
            && self.frame.is_none()
            && self.index.is_none()
        {
            return;
        }
        tracing::debug!("[cpu-onion] 停用 CPU 洋葱皮（释放渲染器与文档快照，保留版本序列）");
        let version = self.version;
        *self = Self::default();
        self.version = version;
    }

    /// 当前帧（诊断用）。
    #[cfg(test)]
    pub(crate) fn frame(&self) -> Option<&CpuOnionFrame> {
        self.frame.as_ref()
    }

    /// 最近一次光栅实际绘制的音符数（诊断用；可区分整帧与窄条路径）。
    #[cfg(test)]
    pub(crate) fn visible_notes(&self) -> u64 {
        self.renderer
            .as_ref()
            .map_or(0, CpuOnionRenderer::visible_notes)
    }

    /// 当前缩放是否可能启用彩色覆盖光栅（基础 bin 8 tick × 2px 粗粒度）。
    ///
    /// 这里只做保守预判：真正的可用性还要看覆盖层 key 数/构建结果。返回 true
    /// 时按整帧路径规划，避免覆盖层与 Region 像素混合。
    fn coverage_zoom(&self, camera: &lumino_cpu_onion::CanvasCamera) -> bool {
        camera.zoom_x.is_finite() && camera.zoom_x > 0.0 && camera.zoom_x <= 2.0 / 8.0
    }

    /// 生成/复用 CPU 洋葱皮帧。
    ///
    /// `muted` 长度 = 文档轨数（越界缺省 `false`）；`current_track` 越界时渲染器
    /// 按「非当前」处理（所有轨都是洋葱皮）。`dirty` / `dirty_tracks` 来自
    /// [`lumino_editor_state::EditorData`]（Host 每帧 take/clone）。
    pub(crate) fn update(
        &mut self,
        key: CpuOnionFrameKey,
        document: &MidiDocument,
        muted: &[bool],
        dirty: Option<CpuOnionDirty>,
        dirty_tracks: Option<&HashSet<usize>>,
    ) -> CpuOnionOutput {
        let layout_changed = self.frozen.is_none_or(|frozen| !frozen.layout_eq(&key));
        let gen_changed = self.index_gen != Some(key.track_notes_gen);
        // 「代次变了但没有任何原子变更记录」= 来源只能是直接 `track_notes_mut`
        // 编辑（拖动 / 批量编辑 / 连奏，或协作远端 / 未来的直写路径）。
        // 把它们全部登记为几何待补：当前轨等切轨再补（热路径不重算），
        // 其它轨（含静音轨）在下一次 `apply_incremental` 当场补齐。
        if gen_changed && dirty.as_ref().is_none_or(|dirty| dirty.changes.is_empty()) {
            self.register_stale_tracks(dirty_tracks, key.current_track);
        }
        let stale_needs_repair = self
            .stale_tracks
            .iter()
            .any(|track| *track != key.current_track);
        let scroll_dx = self.scroll_dx(&key);
        let plan = plan_update(PlanInput {
            frame_ready: self.frame.is_some(),
            index_ready: self.index.is_some() && self.document.is_some(),
            layout_changed,
            scroll_dx,
            gen_changed,
            stale_needs_repair,
            dirty: dirty.as_ref(),
            dirty_tracks,
            current_track: key.current_track,
            muted,
            camera: &key.geometry.camera,
            frame_size: (key.geometry.width, key.geometry.height),
        });

        self.maintain_index(
            document,
            &plan.index,
            dirty.as_ref(),
            key.current_track,
            key.track_notes_gen,
        );
        if plan.index != CpuOnionIndex::Keep {
            self.index_gen = Some(key.track_notes_gen);
        }

        // 简化层（彩色覆盖/热力图）逐屏幕列重采样，单音符脏区覆盖不了受影响的
        // 整列；且覆盖统计覆盖全部音轨，当前轨编辑也会改简化层 → 代次一变就必须
        // 整帧。每帧都重算档位，复用帧也必须保持 `overview_active`，否则空闲
        // 一帧就会误判「退出总览」。
        let overview = self
            .index
            .as_ref()
            .is_some_and(|index| lumino_cpu_onion::uses_overview(index, &key.geometry.camera));
        // 彩色覆盖光栅在 <30 小节也会接管；它和热力图一样是整帧重采样，绝不能
        // 让单音符 Region/Scroll 路径去改（否则会留下 Exact 像素与覆盖块混合）。
        let coverage_zoom = self.coverage_zoom(&key.geometry.camera);
        let simplified = overview || coverage_zoom;
        let mode_changed = simplified != self.simplified_active;
        self.simplified_active = simplified;
        let redraw = overview_redraw(simplified, mode_changed, gen_changed, plan.redraw);

        let Some(redraw) = redraw else {
            // 不重绘：把冻结键推到新代次，避免下一帧重复判断
            // （几何待补已在入口按「上一帧当前轨」登记）
            self.frozen = Some(key);
            return CpuOnionOutput {
                frame: self.reuse_frame(),
                rebuilt: false,
                dirty: None,
            };
        };

        let (Some(snapshot), Some(index)) = (self.document.clone(), self.index.clone()) else {
            // 防御：索引/快照缺失（正常路径不会到达）
            self.frame = None;
            self.frozen = None;
            return CpuOnionOutput {
                frame: None,
                rebuilt: false,
                dirty: None,
            };
        };

        let geometry = key.geometry;
        let track_count = snapshot.track_count().max(1);
        // 密度/覆盖金字塔只在真正可能用到时整建：总览档，或缩放到基础 bin
        // （8 tick）已不足 1px 的密集区间。后者正是 Exact 逐音符扫描会卡死的
        // 区间，彩色覆盖光栅在这里代替热力图/逐音符扫描。
        let density = if simplified {
            self.density.ensure(
                key.track_notes_gen,
                snapshot.as_ref(),
                index.as_ref(),
                geometry.camera.key_count,
                worker_bands(),
            )
        } else {
            None
        };
        // 调色板或轨数变化都要重建每轨颜色缓存（实例颜色固化在像素里）
        let refresh_palette =
            self.palette_idx != Some(key.palette_idx) || plan.index == CpuOnionIndex::Rebuild;
        let (pixels, dirty) = {
            let renderer = self
                .renderer
                .get_or_insert_with(|| CpuOnionRenderer::new(worker_bands(), track_count));
            renderer.resize(geometry.width, geometry.height);
            if refresh_palette {
                renderer.refresh_palette(track_count);
            }
            let dirty = match redraw {
                CpuOnionRedraw::Full => {
                    renderer.render_full_with_density(
                        snapshot,
                        index,
                        &geometry.camera,
                        key.current_track,
                        muted,
                        density,
                    );
                    // 整帧重绘：脏区语义交给「全量上传」，不标注局部范围
                    None
                }
                CpuOnionRedraw::Region(rect) => {
                    renderer.render_region(
                        snapshot,
                        index,
                        &geometry.camera,
                        key.current_track,
                        muted,
                        rect,
                    );
                    Some(rect)
                }
                CpuOnionRedraw::Scroll(dx) => {
                    // 整数像素平移已有帧，只重画新露出的窄条；纹理整体位移，
                    // 必须全量上传（`dirty = None`）。
                    renderer.shift_frame(dx);
                    let width = geometry.width;
                    let (x0, x1) = if dx > 0 {
                        (width.saturating_sub(dx as u32), width)
                    } else {
                        (0, dx.unsigned_abs())
                    };
                    renderer.render_strip(
                        snapshot,
                        index,
                        &geometry.camera,
                        key.current_track,
                        muted,
                        x0,
                        x1,
                    );
                    None
                }
            };
            // 帧像素由 Arc 共享：快照只克隆 Arc；渲染器下一次写回时由
            // `Arc::make_mut` 做写时复制，旧快照保持不可变。
            let pixels: Arc<Vec<u8>> = Arc::clone(&renderer.frame().pixels);
            (pixels, dirty)
        };

        self.version = self.version.wrapping_add(1).max(1);
        self.palette_idx = Some(key.palette_idx);
        let frame = CpuOnionFrame::new(
            geometry.x,
            geometry.y,
            geometry.width,
            geometry.height,
            pixels,
            self.version,
        )
        // 脏区透传：`Some` 让 gfx 只重写该子矩形，`None` 走全量上传
        .with_dirty_rect(dirty.map(to_gfx_rect));
        self.frame = Some(frame.clone());
        self.frozen = Some(key);
        CpuOnionOutput {
            frame: Some(frame),
            rebuilt: true,
            dirty,
        }
    }

    /// 复用上一帧：**清掉旧脏区**后返回克隆。
    ///
    /// 脏区语义只属于「刚重绘的那一帧」：复用帧的 `version` 没变，若把旧脏区
    /// 继续带下去，下游可能把它当成本帧的增量范围。gfx 侧同版本本来就会
    /// `Reuse`，但字段语义必须干净（也清掉驻留副本，`frame()` 诊断同样受益）。
    fn reuse_frame(&mut self) -> Option<CpuOnionFrame> {
        let frame = self.frame.as_mut()?;
        frame.dirty_rect = None;
        Some(frame.clone())
    }

    /// 登记「索引几何待补」的音轨（仅在无原子变更可表达时调用）。
    ///
    /// - `Some(empty)`：约定为「只有当前轨变了」（`record_update_ranges` 用空集
    ///   表达拖动热路径）→ 登记**上一帧**的当前轨；
    /// - `Some(set)`：登记集合里的每一条。**包括静音轨**——取消静音后它就是洋葱轨，
    ///   届时布局变化只重绘、不重建索引，几何过期会静默漏画长音符；
    /// - `None`：来源未知，调用方走整帧 + 整索引重建，无需登记。
    fn register_stale_tracks(
        &mut self,
        dirty_tracks: Option<&HashSet<usize>>,
        current_track: usize,
    ) {
        match dirty_tracks {
            None => {}
            Some(tracks) if tracks.is_empty() => {
                let previous = self
                    .frozen
                    .map_or(current_track, |frozen| frozen.current_track);
                self.stale_tracks.insert(previous);
            }
            Some(tracks) => self.stale_tracks.extend(tracks.iter().copied()),
        }
    }
}
