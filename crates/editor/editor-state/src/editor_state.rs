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
pub mod drag_state;
pub mod editor_data;
pub mod hit_test;
pub mod image_to_midi;
pub mod image_to_midi_material;
pub mod interaction_ops;
pub mod interaction_state;
pub mod line_tool;
pub mod note_grouping;
pub mod shape_tool;
pub mod text_tool;
pub mod viewport;

pub use canvas_state::CanvasState;
pub use constants::{
    DEFAULT_BPM, DEFAULT_PREVIEW_VELOCITY, GLUE_PROXIMITY_THRESHOLD, SELECTION_BOX_EDGE_THRESHOLD,
};
pub use drag_state::DragState;
pub use editor_data::{EditorData, NoteDeltaEvent};
pub use image_to_midi::{
    I2mInteraction, ImageToMidiMode, ImageToMidiPreview, ImageToMidiState, PreviewNote, RegionRect,
};
pub use interaction_state::{
    EditState, HitType, InteractionState, PreviewSequenceNote, SelectionHitType,
};
pub use line_tool::{
    BezierAnchor, HandleSide, LinePath, LineToolInteraction, LineToolState, PathSnapshot,
};
pub use shape_tool::{
    ShapeInstance, ShapeKind, ShapeToolInteraction, ShapeToolState,
};

use std::collections::HashSet;

use lumino_core::Tool;
use lumino_core::storage::config::AutoScrollConfig;
use lumino_core::view_state::ViewState;

/// 横向视图备份（纵向切回横向时恢复，避免音符因缩放/滚动错位而“消失”）
#[derive(Debug, Clone)]
pub struct HorizontalViewBackup {
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub max_scroll: (f32, f32),
}

/// 编辑器完整状态
#[derive(Debug)]
pub struct EditorState {
    /// 视图状态
    pub view: ViewState,
    /// 画布状态
    pub canvas: CanvasState,
    /// 交互状态机
    pub interaction: InteractionState,
    /// 当前选中的工具
    pub tool: Tool,
    /// 自动滚动配置
    pub auto_scroll: AutoScrollConfig,
    /// 最大滚动范围（横向、纵向）
    pub max_scroll: (f32, f32),
    /// 当前是否为纵向卷帘视图（影响自动滚动轴向与播放指示线方向）
    pub is_vertical_roll: bool,
    /// 横向视图备份（进入纵向前保存，退出时恢复）
    pub horizontal_backup: Option<HorizontalViewBackup>,
    /// 编辑器文档与音符数据
    pub data: EditorData,
    /// 图片转 MIDI 放置模式状态
    pub image_to_midi: image_to_midi::ImageToMidiState,
    /// 曲线工具直线绘制状态
    pub line_tool: line_tool::LineToolState,
    /// 形状工具绘制状态（矩形/圆/三角 拉框）
    pub shape_tool: shape_tool::ShapeToolState,
    /// 文字工具状态（文本框 + 输入文字 + 采样模式）
    pub text_tool: text_tool::TextToolState,
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
            is_vertical_roll: false,
            horizontal_backup: None,
            image_to_midi: image_to_midi::ImageToMidiState::default(),
            line_tool: line_tool::LineToolState::default(),
            shape_tool: shape_tool::ShapeToolState::default(),
            text_tool: text_tool::TextToolState::new(),
        }
    }

    /// 重置编辑器状态到初始值（释放所有内存）
    pub fn reset(&mut self) {
        self.data.reset();
        self.interaction = InteractionState::default();
        self.view = ViewState::default();
        self.tool = Tool::Pointer;
        self.auto_scroll = AutoScrollConfig::default();
        self.horizontal_backup = None;
        self.image_to_midi = image_to_midi::ImageToMidiState::default();
        self.line_tool = line_tool::LineToolState::default();
        self.shape_tool = shape_tool::ShapeToolState::default();
        self.text_tool = text_tool::TextToolState::new();
        let total_ticks = self.view.total_ticks;
        viewport::Viewport::new(&mut self.view, &mut self.max_scroll)
            .update_max_scroll(total_ticks);
    }

    /// 保存横向视图备份（进入纵向前调用，仅首次保存）
    pub fn save_horizontal_backup(&mut self) {
        if self.horizontal_backup.is_some() {
            return;
        }
        self.horizontal_backup = Some(HorizontalViewBackup {
            zoom_x: self.view.zoom_x,
            zoom_y: self.view.zoom_y,
            scroll_x: self.view.scroll_x,
            scroll_y: self.view.scroll_y,
            max_scroll: self.max_scroll,
        });
    }

    /// 恢复横向视图备份（退出纵向时调用）
    pub fn restore_horizontal_backup(&mut self) {
        if let Some(backup) = self.horizontal_backup.take() {
            self.view.zoom_x = backup.zoom_x;
            self.view.zoom_y = backup.zoom_y;
            self.view.scroll_x = backup.scroll_x;
            self.view.scroll_y = backup.scroll_y;
            self.max_scroll = backup.max_scroll;
            self.view.smooth_scroll.target_x = backup.scroll_x;
            self.view.smooth_scroll.target_y = backup.scroll_y;
            self.view.smooth_scroll.active = false;
            // 重算 max_scroll 以同步画布尺寸变化
            let total_ticks = self.view.total_ticks;
            viewport::Viewport::new(&mut self.view, &mut self.max_scroll)
                .update_max_scroll(total_ticks);
            // 钳制 scroll 到新 max 范围内
            self.view.scroll_x = self.view.scroll_x.clamp(0.0, self.max_scroll.0);
            self.view.scroll_y = self.view.scroll_y.clamp(0.0, self.max_scroll.1);
            self.view.smooth_scroll.target_x = self.view.scroll_x;
            self.view.smooth_scroll.target_y = self.view.scroll_y;
        }
    }

    /// 设置当前工具
    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        if tool != Tool::Pointer {
            self.interaction.selected_notes.clear();
        }
        // 图片转 MIDI 放置模式：切换工具仅清除区域框（保留预览，可重新框选）
        if tool != Tool::PointerYSelect && self.image_to_midi.is_active() {
            self.image_to_midi.clear_region();
            self.interaction.edit_state = interaction_state::EditState::Idle;
            self.interaction.selected_notes.clear();
        }
        // 曲线工具直线模式：切换工具清除直线状态（避免残留干扰其他工具）
        if tool != Tool::Curve {
            self.line_tool.reset();
        }
        // 文字工具：切换走时清除文本框与输入状态
        if tool != Tool::Text {
            self.text_tool.reset();
        }
        // 形状工具：切换走时清除拉框状态（保留图形类型，详见 clear_pending）
        if tool != Tool::Shape {
            self.shape_tool.clear_pending();
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
        assert_eq!(state.data.current_track_note_count(), 0);
    }

    #[test]
    fn test_editor_state_reset() {
        let mut state = EditorState::new();
        state.tool = Tool::Eraser;
        state.data =
            EditorData::with_f32_notes(0, &[lumino_note_core::note::Note::new(0.0, 60, 1.0)]);
        state.reset();
        assert_eq!(state.tool, Tool::Pointer);
        assert_eq!(state.data.current_track_note_count(), 0);
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
