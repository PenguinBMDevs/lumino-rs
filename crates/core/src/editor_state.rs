//! 编辑器状态与业务逻辑
//!
//! 本模块将原先单一 God Object 文件拆分为内聚子模块：
//!
//! - `constants`: 编辑器相关常量
//! - `canvas_state`: Canvas 几何状态
//! - `interaction_state`: 交互状态机
//! - `interaction_ops`: 交互业务逻辑
//! - `editor_data`: 音符数据与音轨缓存
//! - `note_grouping`: 音符合并分组算法
//! - `hit_test`: 碰撞检测与选择框几何计算
//! - `viewport`: 视口滚动、缩放与可见键数量管理
//!
//! `EditorState` 只保留结构定义、生命周期方法以及真正跨领域的协调逻辑。
//! 其他具体操作请直接使用各子模块的 API。

pub mod canvas_state;
pub mod constants;
pub mod editor_data;
pub mod hit_test;
pub mod interaction_ops;
pub mod interaction_state;
pub mod note_grouping;
pub mod viewport;

pub use canvas_state::CanvasState;
pub use constants::{
    DEFAULT_BPM, DEFAULT_PREVIEW_VELOCITY, GLUE_PROXIMITY_THRESHOLD, SELECTION_BOX_EDGE_THRESHOLD,
};
pub use editor_data::EditorData;
pub use interaction_state::{EditState, HitType, InteractionState, SelectionHitType};

use std::collections::HashSet;

use crate::Tool;
use crate::storage::config::AutoScrollConfig;
use crate::view_state::ViewState;

/// 编辑器完整状态
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
        let mut view = ViewState::default();
        let total_ticks = view.total_ticks;
        let mut max_scroll = (0.0, 0.0);
        viewport::Viewport::new(&mut view, &mut max_scroll).update_max_scroll(total_ticks);
        Self {
            view,
            max_scroll,
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
        let total_ticks = self.view.total_ticks;
        viewport::Viewport::new(&mut self.view, &mut self.max_scroll)
            .update_max_scroll(total_ticks);
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        state
            .data
            .notes
            .push_back(crate::note::Note::new(0.0, 60, 1.0));
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
}
