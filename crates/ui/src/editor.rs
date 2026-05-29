pub mod editor_state;
pub mod grid;
pub mod history;
pub mod note;
pub mod onion_bg_pool;
pub mod onion_skin;
pub mod recording;
pub mod scrollbar_widget;
pub mod smooth_scroll;
pub mod spatial_index;
pub mod velocity;

// 子模块
mod auto_scroll;
mod clipboard;
mod coords;
mod drag;
mod interaction;
mod note_ops;
mod onion_skin_editor;
mod onion_skin_ops;
mod rendering;
mod scroll;
mod settings;
mod track;

#[cfg(test)]
mod tests;

use crate::{message::AudioAction, toolbar::Tool};
use iced_core::Point;
use iced_widget::canvas;
use std::cell::{Cell, RefCell};

use note::Note;
pub use onion_bg_pool::*;
pub use onion_skin::OnionSkinConfig;
// 统一从 editor_state 导入（重构迁移）
pub use editor_state::{EditState, HitType, SelectionHitType, ViewState};

/// 缓存失效标志位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheInvalidation(u8);

impl CacheInvalidation {
    pub const NONE: Self = Self(0);
    pub const GRID: Self = Self(1 << 0);
    pub const KEYBOARD: Self = Self(1 << 1);
    pub const RULER: Self = Self(1 << 2);
    pub const ALL: Self = Self(0b111);
}

/// 钢琴卷帘编辑器
pub struct Editor {
    pub grid_cache: canvas::Cache<crate::Renderer>,
    /// 键盘缓存（只随垂直滚动变化）
    pub keyboard_cache: canvas::Cache<crate::Renderer>,
    /// 标尺缓存（只随水平滚动变化）
    pub ruler_cache: canvas::Cache<crate::Renderer>,

    /// 洋葱皮配置
    onion_skin_config: OnionSkinConfig,

    /// 协作远端用户光标信息（用户ID -> (位置, 颜色, 用户名)）
    pub remote_cursors: std::collections::HashMap<String, (Point, String, String)>,

    /// 演奏指示线位置（以 tick 为单位）
    pub playback_position: f32,

    /// 循环区域状态
    pub loop_range: Option<grid::LoopRange>,

    /// 音符数据是否已变化（需要更新播放管理器）
    notes_changed: bool,

    /// 音符空间索引（惰性更新）
    pub note_index: RefCell<Option<spatial_index::NoteSpatialIndex>>,
    pub note_index_dirty: Cell<bool>,
    pub query_cache: RefCell<Vec<usize>>,

    /// 其他音轨的音符空间索引（用于洋葱皮等，懒加载）
    pub track_note_indices:
        RefCell<std::collections::HashMap<usize, spatial_index::NoteSpatialIndex>>,

    /// 统一状态管理
    pub editor_state: editor_state::EditorState,

    /// 力度编辑面板
    pub velocity_panel: velocity::VelocityPanel,

    /// 缓存的可见音轨索引（洋葱皮用），避免每帧重复计算
    pub cached_onion_track_indices: Vec<usize>,
    /// 缓存的可见音轨哈希值（不含颜色）
    pub cached_onion_track_hash: u64,
    /// 缓存的音轨颜色哈希值
    pub cached_onion_config_hash: u64,
    /// 缓存是否有效
    pub onion_cache_valid: bool,

    /// 框选框的动画显示状态（用于弹簧物理动画）
    pub selection_box_anim: RefCell<Option<SelectionBoxAnimState>>,
}

/// 框选框弹簧动画状态
#[derive(Debug, Clone, Copy)]
pub struct SelectionBoxAnimState {
    /// 起点的屏幕坐标（固定）
    pub start_pos: Point,
    /// 当前动画显示的终点坐标（弹簧末端）
    pub current_pos: Point,
    /// 当前速度（用于弹簧物理）
    pub velocity: Point,
    /// 上一次吸附的目标 tick（用于判断是否需要更新弹簧目标）
    pub snapped_tick: f32,
    /// 上一次吸附的目标 key
    pub snapped_key: u16,
    /// 弹簧是否已收敛到目标位置（AnimationTick 循环用）
    pub converged: bool,
}

/// 编辑器各组件的内存占用快照（字节）
#[derive(Debug, Clone, Default)]
pub struct EditorMemory {
    /// editor.notes 的估算内存（len × sizeof(Note) + 树形结构开销）
    pub notes_bytes: usize,
    /// track_notes HashMap 中所有 im::Vector 的音符总量
    pub track_notes_count: usize,
    pub track_notes_bytes: usize,
    /// track_notes 的条目数
    pub track_notes_entries: usize,
    /// document Arc 指向事件的 Vec 内存
    pub document_events_bytes: usize,
    /// 空间索引追踪条目数
    pub track_note_indices_entries: usize,
}

impl Editor {
    /// 收集编辑器各组件的内存占用快照
    pub fn memory_breakdown(&self) -> EditorMemory {
        let d = &self.editor_state.data;
        let note_size = std::mem::size_of::<Note>();

        // editor.notes
        let notes_len = d.notes.len();
        let notes_bytes = notes_len * note_size;

        // track_notes
        let track_notes_entries = d.track_notes.len();
        let mut track_notes_count = 0usize;
        let mut track_notes_bytes = 0usize;
        for notes in d.track_notes.values() {
            track_notes_count += notes.len();
            track_notes_bytes += notes.len() * note_size;
        }

        // document events (CompactEvent=12B, (u32,f32)=8B)
        let doc_is_some = d.document.is_some();
        let doc_event_cap = d
            .document
            .as_ref()
            .map(|d| d.events.capacity())
            .unwrap_or(0);
        let doc_events_bytes = d
            .document
            .as_ref()
            .map(|doc| {
                doc.events.capacity() * 12       // CompactEvent
                    + doc.tempo_changes.capacity() * 8 // (u32, f32)
            })
            .unwrap_or(0);

        // track_note_indices
        let track_note_indices_entries = self.track_note_indices.borrow().len();

        tracing::info!(
            "[MEMORY_DEBUG] document={}, events_cap={}, notes_len={}, track_notes_entries={}, track_notes_count={}",
            doc_is_some,
            doc_event_cap,
            notes_len,
            track_notes_entries,
            track_notes_count,
        );

        EditorMemory {
            notes_bytes,
            track_notes_count,
            track_notes_bytes,
            track_notes_entries,
            document_events_bytes: doc_events_bytes,
            track_note_indices_entries,
        }
    }

    pub fn new() -> Self {
        Self {
            editor_state: editor_state::EditorState::new(),
            grid_cache: canvas::Cache::new(),
            keyboard_cache: canvas::Cache::new(),
            ruler_cache: canvas::Cache::new(),
            onion_skin_config: OnionSkinConfig::new(),
            remote_cursors: std::collections::HashMap::new(),
            playback_position: 0.0,
            loop_range: Some(grid::LoopRange::new()),
            notes_changed: false,
            note_index: RefCell::new(None),
            note_index_dirty: Cell::new(true),
            query_cache: RefCell::new(Vec::new()),
            track_note_indices: RefCell::new(std::collections::HashMap::new()),
            velocity_panel: velocity::VelocityPanel::new(),
            cached_onion_track_indices: Vec::new(),
            cached_onion_track_hash: 0,
            cached_onion_config_hash: 0,
            onion_cache_valid: false,
            selection_box_anim: RefCell::new(None),
        }
    }

    /// 设置当前工具（委托到 editor_state）
    pub fn set_tool(&mut self, tool: Tool) {
        self.editor_state.set_tool(tool);
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.editor_state.tool
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    ) {
        self.remote_cursors.insert(
            user_id.to_string(),
            (Point::new(x, y), color.to_string(), username.to_string()),
        );
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: &str) {
        self.remote_cursors.remove(user_id);
        self.grid_cache.clear();
    }

    /// 更新鼠标位置（由外部调用）
    pub fn update_cursor_position(&mut self, position: Option<Point>) {
        self.editor_state.update_cursor_position(position);
    }

    /// 更新 Canvas 偏移量（用于坐标转换）
    pub fn set_canvas_offset(&mut self, offset: Point) {
        self.editor_state.set_canvas_offset(offset);
    }

    /// 更新 Canvas 尺寸
    pub fn set_canvas_size(&mut self, size: Point) {
        self.editor_state.set_canvas_size(size);
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        let actions = self.editor_state.interaction.take_audio_actions();
        if !actions.is_empty() {
            tracing::debug!("Editor: 取出了 {} 个音频动作", actions.len());
        }
        actions
    }

    /// 设置总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: u32) {
        self.editor_state.view.total_ticks = total_ticks;
        let max_scroll_x = total_ticks as f32 * self.editor_state.view.zoom_x;
        self.editor_state.max_scroll.x = max_scroll_x;
    }

    /// 设置 PPQ
    pub fn set_ppq(&mut self, ppq: u16) {
        self.editor_state.view.ppq = ppq;
    }

    /// 检查音符数据是否已变化
    pub fn notes_changed(&self) -> bool {
        self.notes_changed
    }

    /// 清除音符变化标志
    pub fn clear_notes_changed(&mut self) {
        self.notes_changed = false;
    }

    /// 统一缓存失效（替代散落的 grid_cache.clear() 等调用）
    #[inline]
    pub fn invalidate_caches(&mut self, which: CacheInvalidation) {
        if which.0 & CacheInvalidation::GRID.0 != 0 {
            self.grid_cache.clear();
        }
        if which.0 & CacheInvalidation::KEYBOARD.0 != 0 {
            self.keyboard_cache.clear();
        }
        if which.0 & CacheInvalidation::RULER.0 != 0 {
            self.ruler_cache.clear();
        }
    }

    /// 标记音符数据已变化
    pub fn mark_notes_changed(&mut self) {
        self.notes_changed = true;
        self.note_index_dirty.set(true);
        self.track_note_indices
            .borrow_mut()
            .remove(&self.editor_state.data.current_track);
    }

    /// 重置编辑器内部状态到默认值（释放私有字段内存）
    ///
    /// 供 `clear_editor()` 调用，重置本模块私有的字段：
    /// - `onion_skin_config`：洋葱皮配置
    /// - `notes_changed`：音符变更标志
    /// - `playback_position`：播放指示线位置
    pub fn reset_internal_state(&mut self) {
        use crate::editor::onion_skin::OnionSkinConfig;
        self.onion_skin_config = OnionSkinConfig::new();
        self.notes_changed = false;
        self.playback_position = 0.0;
        self.velocity_panel = velocity::VelocityPanel::new();
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// Push current state to history
    pub fn push_history(&mut self) {
        let d = &self.editor_state.data;
        let snapshot = history::EditorSnapshot::new(d.notes.clone(), d.current_track);
        tracing::debug!(
            "推送历史记录: {} 个音符，音轨 {}",
            snapshot.notes.len(),
            snapshot.current_track
        );
        self.editor_state.data.history.push(snapshot);
    }

    /// Undo the last action
    pub fn undo(&mut self) -> bool {
        let d = &self.editor_state.data;
        let current_state = history::EditorSnapshot::new(d.notes.clone(), d.current_track);
        tracing::info!(
            "尝试撤销: 当前音符数 = {}, 可撤销 = {}",
            d.notes.len(),
            self.can_undo()
        );

        if let Some(snapshot) = self.editor_state.data.history.undo(current_state) {
            self.editor_state.data.notes = snapshot.notes;
            self.editor_state.data.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!(
                "撤销操作成功: {} 个音符",
                self.editor_state.data.notes.len()
            );
            true
        } else {
            tracing::info!("没有可撤销的操作");
            false
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> bool {
        let d = &self.editor_state.data;
        let current_state = history::EditorSnapshot::new(d.notes.clone(), d.current_track);

        if let Some(snapshot) = self.editor_state.data.history.redo(current_state) {
            self.editor_state.data.notes = snapshot.notes;
            self.editor_state.data.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("重做操作成功");
            true
        } else {
            tracing::info!("没有可重做的操作");
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.editor_state.data.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.editor_state.data.history.can_redo()
    }

    /// 更新框选框的弹簧物理动画
    ///
    /// 使用弹簧物理模拟让选择框边界产生 Q 弹的弹性效果。
    /// 以 snap_precision 为精度单位"跳跃"，在跳跃之间使用弹簧动画过渡：
    /// - 鼠标移动时，先计算吸附到网格的目标位置
    /// - 只有当吸附位置发生变化时，才更新弹簧目标
    /// - 弹簧以弹性方式从上一个吸附位置过渡到新的吸附位置
    /// - 弹簧收敛后标记 converged，供 frame.rs 停止 AnimationTick 轮询
    ///
    /// `mouse_pos`:
    /// - `Some(pos)`: 鼠标移动中，重新计算吸附目标
    /// - `None`: 持续推进弹簧物理向现有目标收敛（用于 AnimationTick）
    pub(crate) fn update_selection_box_animation(&self, mouse_pos: Option<Point>) {
        use crate::editor::EditState;
        use crate::editor::SelectionBoxAnimState;

        let interaction = &self.editor_state.interaction;

        match interaction.edit_state {
            EditState::Selecting {
                start_tick,
                start_key,
                current_tick,
                current_key,
                ..
            } => {
                // 计算起点的屏幕坐标（固定锚点）
                let start_x = self.tick_to_x(start_tick);
                let start_y = self.key_to_y(start_key);
                let start_pos = Point::new(start_x, start_y);

                // 计算吸附后的目标位置
                let snapped_tick = if let Some(pos) = mouse_pos {
                    let tick = self.x_to_tick(pos.x);
                    self.snap_tick(tick)
                } else {
                    current_tick
                };
                let snapped_key = if let Some(pos) = mouse_pos {
                    self.y_to_key(pos.y)
                } else {
                    current_key
                };

                // 获取或初始化动画状态
                let mut anim = self.selection_box_anim.borrow_mut();

                let (display_current, mut velocity, last_snapped_tick, last_snapped_key) =
                    if let Some(state) = *anim {
                        (
                            state.current_pos,
                            state.velocity,
                            state.snapped_tick,
                            state.snapped_key,
                        )
                    } else {
                        // 初始状态：显示位置等于第一个吸附位置
                        let init_x = self.tick_to_x(snapped_tick);
                        let init_y = self.key_to_y(snapped_key);
                        (
                            Point::new(init_x, init_y),
                            Point::new(0.0, 0.0),
                            snapped_tick,
                            snapped_key,
                        )
                    };

                // 判断吸附位置是否发生变化
                let snapped_changed =
                    snapped_tick != last_snapped_tick || snapped_key != last_snapped_key;

                // 计算弹簧目标位置：吸附位置变化时更新目标，否则保持上一次的目标
                let spring_target = if snapped_changed {
                    let target_x = self.tick_to_x(snapped_tick);
                    let target_y = self.key_to_y(snapped_key);
                    Point::new(target_x, target_y)
                } else {
                    let target_x = self.tick_to_x(last_snapped_tick);
                    let target_y = self.key_to_y(last_snapped_key);
                    Point::new(target_x, target_y)
                };

                // 弹簧物理参数（Q弹效果）
                const STIFFNESS: f32 = 400.0; // 弹簧刚度（越大回弹越快）
                const DAMPING: f32 = 15.0; // 阻尼系数（越小越弹）
                const MASS: f32 = 1.0; // 质量
                const DT: f32 = 1.0 / 60.0; // 固定时间步长（假设60fps）
                const SUB_STEPS: i32 = 4; // 每帧子步数，提高稳定性

                let mut current = display_current;

                // 半隐式欧拉积分，多子步提高稳定性
                for _ in 0..SUB_STEPS {
                    let dt = DT / SUB_STEPS as f32;

                    // 计算弹簧力（胡克定律）
                    let displacement_x = spring_target.x - current.x;
                    let displacement_y = spring_target.y - current.y;
                    let spring_force_x = STIFFNESS * displacement_x;
                    let spring_force_y = STIFFNESS * displacement_y;

                    // 计算阻尼力
                    let damping_force_x = DAMPING * velocity.x;
                    let damping_force_y = DAMPING * velocity.y;

                    // 计算加速度（F = ma => a = F/m）
                    let accel_x = (spring_force_x - damping_force_x) / MASS;
                    let accel_y = (spring_force_y - damping_force_y) / MASS;

                    // 更新速度和位置
                    velocity.x += accel_x * dt;
                    velocity.y += accel_y * dt;
                    current.x += velocity.x * dt;
                    current.y += velocity.y * dt;
                }

                // 弹簧收敛判断：位置和速度都足够接近目标时标记收敛
                // 注意：不在此处清除 anim 状态，因为 Selecting 期间 anim 必须存活
                // 收敛标记由 frame.rs 的 has_selection_anim 控制 AnimationTick 循环
                let dx = current.x - spring_target.x;
                let dy = current.y - spring_target.y;
                let dist_sq = dx * dx + dy * dy;
                let speed_sq = velocity.x * velocity.x + velocity.y * velocity.y;
                const POS_THRESHOLD_SQ: f32 = 0.25; // 0.5 像素的平方
                const VEL_THRESHOLD_SQ: f32 = 0.01; // 0.1 像素/帧的平方

                let converged = dist_sq < POS_THRESHOLD_SQ && speed_sq < VEL_THRESHOLD_SQ;

                *anim = Some(SelectionBoxAnimState {
                    start_pos,
                    current_pos: current,
                    velocity,
                    snapped_tick,
                    snapped_key,
                    converged,
                });
            }
            _ => {
                // 非选择状态，清除动画状态
                *self.selection_box_anim.borrow_mut() = None;
            }
        }
    }
}
