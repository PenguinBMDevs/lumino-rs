//! 悬停状态更新与曲线绘制
//!
//! 包含鼠标悬停检测和力度曲线插值计算。

use iced_core::{Point, Size};
use iced_widget::canvas;
use lumino_core::{AutomationEdit, SegmentShape};

use crate::editor_state::ViewState;
use crate::velocity::EditMode;
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::super::super::super::{VelocityPanel, VelocityPoint};
use super::super::super::state::VelocityCanvasState;
use super::publish_velocity;

impl<'a> super::super::super::VelocityCanvas<'a> {
    /// 更新悬停状态
    pub(super) fn update_hover_state(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) {
        match self.edit_mode {
            EditMode::Velocity => {
                self.update_velocity_hover(state, cursor_pos, bounds_size);
            }
            EditMode::Tempo => {
                self.update_tempo_hover(state, cursor_pos, bounds_size);
            }
            _ => {
                // CC / Bend 模式
                if let Some((view, _target, max_val)) = self.automation_view_params(bounds_size)
                    && let Some(lane) = self.current_automation_lane()
                {
                    state.hover_anchor_tick =
                        Self::hit_test_automation_anchor(lane, &view, cursor_pos, max_val);
                }
                state.hover_point_idx = None;
            }
        }
    }

    /// 更新 Velocity 模式悬停状态
    fn update_velocity_hover(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) {
        let points = self.points();
        let view = &self.editor.editor_state.view;
        if points.is_empty() {
            state.hover_point_idx = None;
            return;
        }

        let hover_idx = Self::hit_test(
            &points,
            cursor_pos,
            bounds_size.width,
            bounds_size.height,
            view,
        );
        if hover_idx != state.hover_point_idx {
            state.hover_point_idx = hover_idx;
        }
        state.hover_anchor_tick = None;
    }

    /// 更新 Tempo 模式悬停状态
    fn update_tempo_hover(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) {
        let tempo_points = VelocityPanel::build_tempo_points(self.editor);
        let view = &self.editor.editor_state.view;
        if let Some(idx) = Self::hit_test_tempo_point(
            &tempo_points,
            cursor_pos,
            bounds_size.width,
            bounds_size.height,
            view,
        ) {
            state.tempo_hover_idx = Some(idx);
        } else {
            state.tempo_hover_idx = None;
        }
        state.hover_point_idx = None;
        state.hover_anchor_tick = None;
    }

    /// 更新曲线绘制
    pub(super) fn update_curve_paint(
        state: &mut VelocityCanvasState,
        points: &[VelocityPoint],
        cursor_pos: Point,
        bounds_size: Size,
        view: &ViewState,
        has_selection: bool,
        is_selected: &dyn Fn(usize) -> bool,
    ) -> Option<canvas::Action<Message>> {
        let start_x = state.curve_start_x;
        let current_x = cursor_pos.x;
        let min_x = start_x.min(current_x);
        let max_x = start_x.max(current_x);
        let current_velocity = Self::y_to_velocity(cursor_pos.y, bounds_size.height);
        let start_velocity = state.curve_start_velocity;
        let mut updates: Vec<(usize, u8)> = Vec::new();

        for point in points {
            let point_x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
            if point_x < min_x || point_x > max_x {
                continue;
            }
            if has_selection && !is_selected(point.note_index) {
                continue;
            }

            let t = if (max_x - min_x).abs() < f32::EPSILON {
                1.0
            } else {
                (point_x - min_x) / (max_x - min_x)
            };
            let interp_velocity_f =
                start_velocity as f32 * (1.0 - t) + current_velocity as f32 * t;
            let new_velocity = interp_velocity_f.round().clamp(0.0, 127.0) as u8;

            if point.velocity != new_velocity {
                state.curve_affected.insert(point.note_index, new_velocity);
                updates.push((point.note_index, new_velocity));
            }
        }

        if updates.is_empty() {
            return None;
        }
        Some(publish_velocity(VelocityAction::CurvePaint(updates)))
    }

    // ── 自动化曲线绘制 ──

    /// 处理曲线绘制拖拽
    pub(super) fn handle_curve_draw_drag(
        &self,
        state: &mut VelocityCanvasState,
        view: lumino_gfx::automation::AutomationViewParams,
        target: lumino_core::AutomationTarget,
        max_val: f32,
        track_idx: u16,
        start_tick: u32,
        start_value: u16,
        cursor_pos: Point,
    ) -> Option<canvas::Action<Message>> {
        let current_tick_f = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0);
        let current_tick = current_tick_f as u32;
        let current_value = view
            .y_to_value(cursor_pos.y, max_val)
            .round()
            .clamp(0.0, max_val) as u16;
        state.automation_curve_current = Some((current_tick, current_value));

        if current_tick == start_tick {
            return self.handle_single_click_curve_add(track_idx, &target, current_tick, current_value);
        }

        self.handle_two_point_curve_add(track_idx, &target, start_tick, start_value, current_tick, current_value)
    }

    /// 单点点击：只创建一个 Curve{tension:0} 锚点
    fn handle_single_click_curve_add(
        &self,
        track_idx: u16,
        target: &lumino_core::AutomationTarget,
        tick: u32,
        value: u16,
    ) -> Option<canvas::Action<Message>> {
        Some(publish_velocity(VelocityAction::AutomationEdit(
            AutomationEdit::Add {
                track_idx,
                target: target.clone(),
                channel: 0,
                tick,
                value,
                shape: SegmentShape::Curve { tension: 0 },
            },
        )))
    }

    /// 两点曲线：参考 yinhe 模式创建 2 个锚点（起点 Curve{tension:0} + 终点 Step）
    fn handle_two_point_curve_add(
        &self,
        track_idx: u16,
        target: &lumino_core::AutomationTarget,
        start_tick: u32,
        start_value: u16,
        current_tick: u32,
        current_value: u16,
    ) -> Option<canvas::Action<Message>> {
        let (t1, v1, t2, v2) = if start_tick < current_tick {
            (start_tick, start_value, current_tick, current_value)
        } else {
            (current_tick, current_value, start_tick, start_value)
        };
        Some(publish_velocity(VelocityAction::AutomationBatch(vec![
            AutomationEdit::Add {
                track_idx,
                target: target.clone(),
                channel: 0,
                tick: t1,
                value: v1,
                shape: SegmentShape::Curve { tension: 0 },
            },
            AutomationEdit::Add {
                track_idx,
                target: target.clone(),
                channel: 0,
                tick: t2,
                value: v2,
                shape: SegmentShape::Step,
            },
        ])))
    }
}
