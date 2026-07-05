//! 编辑器状态与业务逻辑
//!
//! 本模块将原先单一 God Object 文件拆分为内聚子模块：
//!
//! - `constants`: 编辑器相关常量
//! - `canvas_state`: Canvas 几何状态
//! - `interaction_state`: 交互状态机
//! - `editor_data`: 音符数据与音轨缓存
//!
//! `EditorState` 作为 facade 聚合以上模块，保留跨领域的协调业务逻辑。

pub mod canvas_state;
pub mod constants;
pub mod editor_data;
pub mod interaction_state;

pub use canvas_state::CanvasState;
pub use constants::{
    DEFAULT_BPM, DEFAULT_PREVIEW_VELOCITY, GLUE_PROXIMITY_THRESHOLD, SELECTION_BOX_EDGE_THRESHOLD,
};
pub use editor_data::EditorData;
pub use interaction_state::{EditState, HitType, InteractionState, SelectionHitType};

use std::collections::HashSet;

use crate::Tool;
use crate::storage::config::{AutoScrollConfig, EraserBehavior, SelectionBoxMode};
use crate::view_state::ViewState;

/// 编辑器完整状态（包含所有业务逻辑）
#[derive(Debug)]
pub struct EditorState {
    pub view: ViewState,
    pub canvas: CanvasState,
    pub interaction: InteractionState,
    pub tool: Tool,
    pub auto_scroll: AutoScrollConfig,
    pub max_scroll: (f32, f32),
    pub data: EditorData,
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorState {
    /// 创建新的编辑器状态
    pub fn new() -> Self {
        let view = ViewState::default();
        Self {
            max_scroll: (
                view.total_ticks as f32 * view.zoom_x,
                view.visible_key_count as f32 * view.zoom_y,
            ),
            view,
            canvas: CanvasState::default(),
            interaction: InteractionState::default(),
            data: EditorData::new(),
            tool: Tool::Pointer,
            auto_scroll: AutoScrollConfig::default(),
        }
    }

    /// 重置编辑器状态到初始值（释放所有内存）
    pub fn reset(&mut self) {
        self.data.reset();
        self.interaction = InteractionState::default();
        self.view = ViewState::default();
        self.tool = Tool::Pointer;
        self.auto_scroll = AutoScrollConfig::default();
        self.max_scroll = (
            self.view.total_ticks as f32 * self.view.zoom_x,
            self.view.visible_key_count as f32 * self.view.zoom_y,
        );
    }

    /// 根据总 tick 数更新最大滚动范围
    pub fn update_max_scroll(&mut self, total_ticks: u32) {
        self.max_scroll = (
            total_ticks as f32 * self.view.zoom_x,
            self.view.visible_key_count as f32 * self.view.zoom_y,
        );
    }

    /// 设置当前工具
    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        if tool != Tool::Pointer {
            self.interaction.selected_notes.clear();
        }
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.tool
    }

    /// 设置水平滚动位置
    pub fn set_scroll_x(&mut self, scroll_x: f32, keyboard_width: f32, canvas_width: f32) {
        let tw = self.view.total_ticks as f32 * self.view.zoom_x;
        let vw = (canvas_width - keyboard_width).max(0.0);
        let ms = (tw - vw).max(0.0);
        self.view.scroll_x = scroll_x.max(0.0).min(ms);
        self.view.smooth_scroll.target_x = self.view.scroll_x;
        self.view.smooth_scroll.active = false;
    }

    /// 设置垂直滚动位置
    pub fn set_scroll_y(&mut self, scroll_y: f32, canvas_height: f32) {
        let th = self.view.visible_key_count as f32 * self.view.zoom_y;
        let vh = (canvas_height - self.view.ruler_height).max(0.0);
        let ms = (th - vh).max(0.0);
        self.view.scroll_y = scroll_y.max(0.0).min(ms);
        self.view.smooth_scroll.target_y = self.view.scroll_y;
        self.view.smooth_scroll.active = false;
    }

    /// 设置水平缩放
    pub fn set_zoom_x(
        &mut self,
        zoom_x: f32,
        fixed_ratio: f32,
        keyboard_width: f32,
        canvas_width: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let old = self.view.zoom_x;
        self.view.zoom_x = zoom_x.clamp(min_zoom, max_zoom);
        let ratio = self.view.zoom_x / old;
        let vw = (canvas_width - keyboard_width).max(0.0);
        let fp = self.view.scroll_x + vw * fixed_ratio;
        self.view.scroll_x = fp * ratio - vw * fixed_ratio;
        self.update_max_scroll(self.view.total_ticks);
        let ms = (self.max_scroll.0 - vw).max(0.0);
        self.view.scroll_x = self.view.scroll_x.max(0.0).min(ms);
    }

    /// 设置垂直缩放
    pub fn set_zoom_y(
        &mut self,
        zoom_y: f32,
        fixed_ratio: f32,
        canvas_height: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let old = self.view.zoom_y;
        self.view.zoom_y = zoom_y.clamp(min_zoom, max_zoom);
        let ratio = self.view.zoom_y / old;
        let vh = canvas_height.max(0.0);
        let fp = self.view.scroll_y + vh * fixed_ratio;
        self.view.scroll_y = fp * ratio - vh * fixed_ratio;
        self.update_max_scroll(self.view.total_ticks);
        let vh2 = (canvas_height - self.view.ruler_height).max(0.0);
        let ms = (self.max_scroll.1 - vh2).max(0.0);
        self.view.scroll_y = self.view.scroll_y.max(0.0).min(ms);
    }

    /// 设置可见键数量
    pub fn set_visible_key_count(
        &mut self,
        count: u16,
        min_count: u16,
        max_count: u16,
        canvas_height: f32,
    ) {
        self.view.visible_key_count = count.clamp(min_count, max_count);
        self.update_max_scroll(self.view.total_ticks);
        let vh = (canvas_height - self.view.ruler_height).max(0.0);
        let ms = (self.max_scroll.1 - vh).max(0.0);
        if self.view.scroll_y > ms {
            self.view.scroll_y = ms;
        }
    }

    /// 设置键盘宽度
    pub fn set_keyboard_width(&mut self, width: f32) {
        self.view.keyboard_width = width.max(0.0);
    }

    /// 设置吸附精度
    pub fn set_snap_precision(&mut self, precision: f32) {
        self.view.snap_precision = precision.max(1.0);
    }

    /// 设置默认音符长度
    pub fn set_default_note_length(&mut self, length: f32) {
        self.view.default_note_length = length.max(1.0);
    }

    /// 设置橡皮擦行为
    pub fn set_eraser_behavior(&mut self, behavior: EraserBehavior) {
        self.view.eraser_behavior = behavior;
    }

    /// 设置选择框模式
    pub fn set_selection_box_mode(&mut self, mode: SelectionBoxMode) {
        self.view.selection_box_mode = mode;
    }

    // ── 碰撞检测 ──

    /// 命中测试：检测坐标是否落在某个音符上
    pub fn hit_test_note(
        &self,
        pos: (f32, f32),
        edge_threshold_px: f32,
    ) -> Option<(usize, HitType)> {
        let tick = self.view.x_to_tick(pos.0);
        let key = self.view.y_to_key(pos.1);
        for (i, note) in self.data.notes.iter().enumerate().rev() {
            if note.key == key && tick >= note.tick && tick <= note.tick + note.length {
                let sd = (tick - note.tick).abs();
                let ed = (tick - (note.tick + note.length)).abs();
                let et = edge_threshold_px / self.view.zoom_x;
                if ed < et {
                    return Some((i, HitType::End));
                }
                if sd < et {
                    return Some((i, HitType::Start));
                }
                return Some((i, HitType::Middle));
            }
        }
        None
    }

    /// 获取选中音符的边界框
    pub fn get_selection_box_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let sel = &self.interaction.selected_notes;
        if sel.is_empty() {
            return None;
        }
        let mut min_t = f32::INFINITY;
        let mut max_te = f32::NEG_INFINITY;
        let mut max_k = u16::MIN;
        let mut min_k = u16::MAX;
        for &i in sel.iter() {
            if let Some(n) = self.data.notes.get(i) {
                min_t = min_t.min(n.tick);
                max_te = max_te.max(n.tick + n.length);
                max_k = max_k.max(n.key);
                min_k = min_k.min(n.key);
            }
        }
        if min_t.is_infinite() {
            return None;
        }
        Some((
            self.view.tick_to_x(min_t),
            self.view.tick_to_x(max_te),
            self.view.key_to_y(max_k),
            self.view.key_to_y(min_k) + self.view.zoom_y,
        ))
    }

    /// 命中测试选择框边界
    pub fn hit_test_selection_box(&self, pos: (f32, f32)) -> Option<SelectionHitType> {
        let (min_x, max_x, min_y, max_y) = self.get_selection_box_bounds()?;
        if pos.0 < min_x || pos.0 > max_x || pos.1 < min_y || pos.1 > max_y {
            return None;
        }
        let et = SELECTION_BOX_EDGE_THRESHOLD;
        let ol = (pos.0 - min_x).abs() < et;
        let orr = (pos.0 - max_x).abs() < et;
        if ol && !orr {
            return Some(SelectionHitType::LeftEdge);
        }
        if orr && !ol {
            return Some(SelectionHitType::RightEdge);
        }
        Some(SelectionHitType::Inside)
    }

    /// 获取选择框内的音符索引列表（委托到 EditorData）
    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        self.data
            .get_notes_in_selection_box(start_tick, start_key, current_tick, current_key)
    }

    /// 计算选择框内的音符索引（委托到 EditorData）
    pub fn compute_selection(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> HashSet<usize> {
        self.data
            .compute_selection(start_tick, start_key, current_tick, current_key)
    }

    // ── 交互业务逻辑 ──

    /// 开始编辑现有音符
    pub fn start_note_edit(&mut self, index: usize, hit_type: HitType, pos: (f32, f32)) {
        match hit_type {
            HitType::Start => {
                self.data.push_history();
                let note = &self.data.notes[index];
                self.interaction.edit_state = EditState::ResizingStart {
                    note_index: index,
                    original_tick: note.tick,
                    original_length: note.length,
                };
            }
            HitType::End => {
                self.data.push_history();
                self.interaction.edit_state = EditState::ResizingEnd { note_index: index };
            }
            HitType::Middle => {
                let note = &self.data.notes[index];
                self.interaction.edit_state = EditState::PendingDrag {
                    note_index: index,
                    start_pos: pos,
                    original_tick: note.tick,
                    original_key: note.key,
                };
                self.interaction
                    .play_note_audio(note.key, DEFAULT_PREVIEW_VELOCITY);
            }
        }
    }

    /// 开始绘制新音符
    pub fn start_drawing(&mut self, snapped_tick: f32, key: u16) {
        self.interaction.edit_state = EditState::Drawing {
            start_tick: snapped_tick,
            key,
            current_tick: snapped_tick,
        };
        self.interaction
            .play_note_audio(key, DEFAULT_PREVIEW_VELOCITY);
    }

    /// 应用音符变化（单音符编辑），返回是否发生了变更
    pub fn apply_note_changes(
        &mut self,
        new_tick: Option<f32>,
        new_key: Option<u16>,
        new_length: Option<f32>,
    ) -> bool {
        let note_index = match self.interaction.edit_state {
            EditState::Dragging { note_index, .. }
            | EditState::ResizingStart { note_index, .. }
            | EditState::ResizingEnd { note_index, .. } => note_index,
            EditState::DraggingSelection { .. }
            | EditState::ResizingSelectionStart { .. }
            | EditState::ResizingSelectionEnd { .. } => return false,
            _ => return false,
        };

        if let Some(note) = self.data.notes.get_mut(note_index) {
            let mut changed = false;
            if let Some(t) = new_tick {
                note.tick = t;
                changed = true;
            }
            if let Some(k) = new_key {
                note.key = k;
                changed = true;
            }
            if let Some(l) = new_length {
                note.length = l;
                changed = true;
            }
            return changed;
        }
        false
    }

    /// 处理删除键按下事件，返回被删除音符的索引
    pub fn handle_delete_pressed(&mut self) -> Option<usize> {
        if let Some((index, _)) = self.interaction.hover_state {
            self.data.delete_note_by_index(index);
            Some(index)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;

    #[test]
    fn test_editor_state_new() {
        let state = EditorState::new();
        assert_eq!(state.tool, Tool::Pointer);
        assert!(state.data.notes.is_empty());
    }

    #[test]
    fn test_editor_state_reset() {
        let mut state = EditorState::new();
        state.tool = Tool::Eraser;
        state.data.notes.push_back(Note::new(0.0, 60, 1.0));
        state.reset();
        assert_eq!(state.tool, Tool::Pointer);
        assert!(state.data.notes.is_empty());
    }

    #[test]
    fn test_set_tool_clears_selection() {
        let mut state = EditorState::new();
        state.interaction.selected_notes.insert(0);
        state.set_tool(Tool::Eraser);
        assert!(state.interaction.selected_notes.is_empty());
    }

    #[test]
    fn test_set_tool_pointer_keeps_selection() {
        let mut state = EditorState::new();
        state.interaction.selected_notes.insert(0);
        state.set_tool(Tool::Pointer);
        assert_eq!(state.interaction.selected_notes.len(), 1);
    }

    #[test]
    fn test_apply_note_changes_dragging() {
        let mut state = EditorState::new();
        state.data.notes.push_back(Note::new(0.0, 60, 1.0));
        state.interaction.edit_state = EditState::Dragging {
            note_index: 0,
            offset_tick: 0.0,
            offset_key: 0,
            last_played_key: 60,
            original_tick: 0.0,
            original_key: 60,
        };
        assert!(state.apply_note_changes(Some(2.0), Some(64), Some(3.0)));
        let note = &state.data.notes[0];
        assert_eq!(note.tick, 2.0);
        assert_eq!(note.key, 64);
        assert_eq!(note.length, 3.0);
    }

    #[test]
    fn test_apply_note_changes_non_edit_state() {
        let mut state = EditorState::new();
        state.data.notes.push_back(Note::new(0.0, 60, 1.0));
        state.interaction.edit_state = EditState::Idle;
        assert!(!state.apply_note_changes(Some(2.0), None, None));
    }

    #[test]
    fn test_handle_delete_pressed() {
        let mut state = EditorState::new();
        state.data.notes.push_back(Note::new(0.0, 60, 1.0));
        state.interaction.hover_state = Some((0, HitType::Middle));
        assert_eq!(state.handle_delete_pressed(), Some(0));
        assert!(state.data.notes.is_empty());
    }

    #[test]
    fn test_handle_delete_pressed_no_hover() {
        let mut state = EditorState::new();
        assert!(state.handle_delete_pressed().is_none());
    }
}
