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
pub mod interaction_ops;
pub mod interaction_state;
pub mod note_grouping;
pub mod viewport;

pub use canvas_state::CanvasState;
pub use constants::{
    DEFAULT_BPM, DEFAULT_PREVIEW_VELOCITY, GLUE_PROXIMITY_THRESHOLD, SELECTION_BOX_EDGE_THRESHOLD,
};
pub use drag_state::DragState;
pub use editor_data::EditorData;
pub use interaction_state::{EditState, HitType, InteractionState, SelectionHitType};

use std::collections::HashSet;

use crate::Tool;
use crate::pitch_bend::PitchBendCurve;
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
    /// 弯音编辑曲线（None=未进入弯音编辑模式）
    pub pitch_bend_curve: Option<PitchBendCurve>,
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
            pitch_bend_curve: None,
        }
    }

    /// 重置编辑器状态到初始值（释放所有内存）
    pub fn reset(&mut self) {
        self.data.reset();
        self.interaction = InteractionState::default();
        self.view = ViewState::default();
        self.tool = Tool::Pointer;
        self.auto_scroll = AutoScrollConfig::default();
        self.pitch_bend_curve = None;
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

    /// 是否处于弯音编辑模式
    pub fn is_pitch_bend_mode(&self) -> bool {
        self.pitch_bend_curve.is_some()
    }

    /// 进入弯音编辑模式
    pub fn enter_pitch_bend_mode(&mut self, base_key: u16, track: u16, channel: u8) {
        self.pitch_bend_curve = Some(PitchBendCurve::new(track, channel, base_key));
        self.tool = Tool::Anchor;
    }

    /// 退出弯音编辑模式，返回曲线数据用于写入事件
    pub fn exit_pitch_bend_mode(&mut self) -> Option<PitchBendCurve> {
        self.pitch_bend_curve.take()
    }

    /// 退出弯音编辑模式并将曲线采样写入 AutomationLane
    ///
    /// - 曲线模式：按小节 1024 份采样（跳过相邻相同值）
    /// - 直线模式：仅锚点位置生成事件
    /// - 尾部延续：最后一个弯音值自动延续至曲目结束
    pub fn exit_pitch_bend_and_commit(&mut self, ticks_per_measure: u32, total_ticks: u32) {
        use crate::automation::{AutomationEvent, AutomationTarget};
        use crate::pitch_bend::BendDrawMode;
        use std::sync::Arc;

        let Some(curve) = self.pitch_bend_curve.take() else {
            return;
        };

        if curve.anchors.is_empty() {
            return;
        }

        let target = AutomationTarget::PitchBend;
        let track = curve.track;
        let channel = curve.channel;

        // 生成事件列表
        let events: Vec<AutomationEvent> = match curve.mode {
            BendDrawMode::Line => {
                // 直线模式：仅锚点位置生成事件
                curve
                    .anchors
                    .iter()
                    .map(|a| AutomationEvent {
                        tick: a.tick,
                        value: (a.value + crate::midi_types::PITCH_BEND_CENTER) as u16,
                        shape: crate::automation::SegmentShape::linear_curve(),
                    })
                    .collect()
            }
            BendDrawMode::Curve => {
                // 曲线模式：按小节 1024 份采样
                let start_tick = curve.anchors.first().map(|a| a.tick).unwrap_or(0);
                let end_tick = total_ticks.max(curve.anchors.last().map(|a| a.tick).unwrap_or(0));
                let samples = curve.sample_to_events(ticks_per_measure, start_tick, end_tick);
                samples
                    .iter()
                    .map(|s| AutomationEvent {
                        tick: s.tick,
                        value: s.value,
                        shape: crate::automation::SegmentShape::linear_curve(),
                    })
                    .collect()
            }
        };

        // 查找或创建 PitchBend lane
        let lane_idx = self.data.find_or_create_automation_lane(track, target);
        let lane = Arc::make_mut(&mut self.data.automation_lanes[lane_idx]);
        lane.channel = channel;
        // 清空旧事件，写入新事件
        lane.events.clear();
        lane.events = events;
        lane.events.sort_by_key(|e| e.tick);

        tracing::info!(
            "弯音编辑退出：写入 {} 个 PitchBend 事件到轨道 {}",
            lane.events.len(),
            track
        );
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
