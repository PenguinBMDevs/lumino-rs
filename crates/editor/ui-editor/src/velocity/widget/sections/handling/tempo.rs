//! Tempo 模式事件处理
//!
//! 包含速度点点击、拖拽、创建/删除等逻辑。

use iced_core::{Point, Size};
use iced_widget::canvas;
use lumino_core::Tool;

use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::super::super::super::{RESIZE_HANDLE_HEIGHT, VelocityPanel};
use super::super::super::drawing::TEMPO_BPM_MIN;
use super::super::super::state::VelocityCanvasState;
use super::publish_velocity;

impl<'a> super::super::super::VelocityCanvas<'a> {
    /// 处理 Tempo 模式下的按钮点击
    pub(super) fn handle_tempo_button_pressed(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let tempo_points = VelocityPanel::build_tempo_points(self.editor);
        let view = &self.editor.editor_state.view;
        let max_bpm = self.editor.velocity_panel.tempo_max_bpm;
        let hit_idx = Self::hit_test_tempo_point(
            &tempo_points,
            cursor_pos,
            bounds_size.width,
            bounds_size.height,
            view,
            max_bpm,
        );

        // 检查是否在绘制区域内
        let in_draw_area = cursor_pos.x >= 0.0
            && cursor_pos.x <= bounds_size.width
            && cursor_pos.y >= RESIZE_HANDLE_HEIGHT
            && cursor_pos.y <= bounds_size.height;
        if !in_draw_area {
            return None;
        }

        self.handle_tempo_tool_action(state, cursor_pos, bounds_size, hit_idx)
    }

    /// 根据当前工具执行 Tempo 操作
    fn handle_tempo_tool_action(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
        hit_idx: Option<usize>,
    ) -> Option<canvas::Action<Message>> {
        match self.editor.current_tool() {
            Tool::Eraser | Tool::DrawEraser => {
                hit_idx.map(|idx| publish_velocity(VelocityAction::TempoDelete(idx)))
            }
            // Tempo 面板的编辑交互统一由 Curve 工具负责：
            // 命中速度点 → 拖拽移动；未命中 → 创建新速度点。
            // Pencil/Pointer 等其他工具不操作 Tempo 面板（仅在钢琴卷帘使用）。
            Tool::Curve => {
                if let Some(idx) = hit_idx {
                    // 点击已有锚点：开始拖拽
                    state.tempo_drag_idx = Some(idx);
                    Some(publish_velocity(VelocityAction::TempoDragStart(idx)))
                } else {
                    // 空白处创建新点（吸附到网格）
                    let tick = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0);
                    let max_bpm = self.editor.velocity_panel.tempo_max_bpm;
                    let bpm = Self::y_to_bpm(cursor_pos.y, bounds_size.height, max_bpm)
                        .clamp(TEMPO_BPM_MIN, max_bpm);
                    Some(publish_velocity(VelocityAction::TempoAdd(tick, bpm)))
                }
            }
            _ => None,
        }
    }

    /// 处理 Tempo 点拖拽移动
    pub(super) fn handle_tempo_drag_move(
        &self,
        _state: &mut VelocityCanvasState,
        drag_idx: usize,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let tempo_points = VelocityPanel::build_tempo_points(self.editor);
        if drag_idx < tempo_points.len() {
            let max_bpm = self.editor.velocity_panel.tempo_max_bpm;
            let bpm = Self::y_to_bpm(cursor_pos.y, bounds_size.height, max_bpm)
                .clamp(TEMPO_BPM_MIN, max_bpm);
            return Some(publish_velocity(VelocityAction::TempoDragMove(
                drag_idx, bpm,
            )));
        }
        None
    }
}
