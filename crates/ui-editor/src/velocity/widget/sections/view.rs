//! 视图绘制方法：Canvas Program 实现（事件分发、绘制、鼠标交互反馈）

use iced_core::{Color, Point, Rectangle, keyboard, mouse};
use iced_wgpu::Geometry as Geom;
use iced_widget::canvas::{self, Frame, Program, path};

use crate::{Message, Renderer, Theme};

use super::super::super::{
    EditMode, PANEL_PADDING_X, RESIZE_HANDLE_HEIGHT, TOOLBAR_HEIGHT, VelocityPanel,
};
use super::super::drawing::{
    automation_node_color, draw_horizontal_lines, draw_scale_labels, draw_tempo_graph,
    draw_vertical_lines,
};
use super::super::state::{AutomationDrag, CtrlEnd, VelocityCanvasState};
use lumino_gfx::automation::AutomationViewParams;

impl Program<Message, Theme, Renderer> for super::super::VelocityCanvas<'_> {
    type State = VelocityCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let bounds_size = bounds.size();

        if !state._initialized {
            state._initialized = true;
        }
        if bounds_size.width <= PANEL_PADDING_X * 2.0 {
            return None;
        }

        let cursor_pos = {
            let pos = cursor.position()?;
            Point::new(pos.x - bounds.x, pos.y - bounds.y)
        };

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                self.handle_button_pressed(state, cursor_pos, &cursor, bounds_size)
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                self.handle_right_button_pressed(state, cursor_pos, bounds_size)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                self.handle_cursor_moved(state, cursor_pos, &cursor, bounds_size)
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                self.handle_button_released(state, bounds_size)
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                self.handle_wheel_scrolled(state, *delta, bounds_size)
            }
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                Self::handle_modifiers_changed(state, *modifiers);
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geom> {
        match self.edit_mode {
            EditMode::Tempo => {
                let mut frame = Frame::new(renderer, bounds.size());
                let view = &self.editor.editor_state.view;
                draw_horizontal_lines(&mut frame, theme, bounds.size(), self.edit_mode);
                draw_vertical_lines(&mut frame, theme, bounds.size(), view);
                draw_scale_labels(&mut frame, theme, bounds.size(), self.edit_mode);
                let tempo_points = VelocityPanel::build_tempo_points(self.editor);
                if !tempo_points.is_empty() {
                    draw_tempo_graph(
                        &mut frame,
                        &tempo_points,
                        bounds.size(),
                        view,
                        self.editor.velocity_panel.automation_line_thickness,
                    );
                }
                vec![frame.into_geometry()]
            }
            _ => {
                let mut frame = Frame::new(renderer, bounds.size());
                let view = &self.editor.editor_state.view;
                draw_vertical_lines(&mut frame, theme, bounds.size(), view);
                draw_horizontal_lines(&mut frame, theme, bounds.size(), self.edit_mode);
                draw_scale_labels(&mut frame, theme, bounds.size(), self.edit_mode);

                // 绘制自动化拖拽 ghost 反馈（参考 yinhe 模式）
                if let Some(drag) = &state.automation_drag {
                    let view_params = AutomationViewParams {
                        panel_height: bounds.size().height + TOOLBAR_HEIGHT,
                        pixels_per_tick: view.zoom_x,
                        scroll_x: view.scroll_x,
                        keyboard_width: view.keyboard_width,
                        value_zoom: self.editor.velocity_panel.value_zoom,
                        value_scroll: self.editor.velocity_panel.value_scroll,
                        panel_offset_x: 0.0,
                        panel_offset_y: 0.0,
                        toolbar_height: TOOLBAR_HEIGHT,
                        line_thickness: self.editor.velocity_panel.automation_line_thickness,
                    };
                    let max_val = 127.0;

                    // 使用自动化节点统一蓝色绘制 ghost 反馈（与主音轨音符视觉一致）
                    let ghost_color = automation_node_color();
                    let ghost_alpha = Color {
                        a: 0.5,
                        ..ghost_color
                    };

                    match drag {
                        AutomationDrag::MoveAnchor { .. } => {
                            // 移动锚点：在当前位置绘制一个半透明圆点
                            if let Some((cur_tick, cur_value)) = state.automation_curve_current {
                                let cur_x = view_params.tick_to_x(cur_tick);
                                let cur_y = view_params.value_to_y(cur_value as f32, max_val);
                                frame.fill(
                                    &path::Path::circle(Point::new(cur_x, cur_y), 5.0),
                                    ghost_alpha,
                                );
                            }
                        }
                        AutomationDrag::CurveDraw {
                            start_tick,
                            start_value,
                        } => {
                            if let Some((cur_tick, cur_value)) = state.automation_curve_current {
                                let cur_x = view_params.tick_to_x(cur_tick);
                                let cur_y = view_params.value_to_y(cur_value as f32, max_val);
                                let start_x = view_params.tick_to_x(*start_tick);
                                let start_y = view_params.value_to_y(*start_value as f32, max_val);

                                // 拖拽范围底色
                                let range_min_x = start_x.min(cur_x);
                                let range_max_x = start_x.max(cur_x);
                                frame.fill_rectangle(
                                    Point::new(range_min_x, 0.0),
                                    iced_core::Size::new(
                                        range_max_x - range_min_x,
                                        bounds.size().height,
                                    ),
                                    Color {
                                        a: 0.08,
                                        ..ghost_color
                                    },
                                );

                                // 起点到当前点的 ghost 线（采样 path，与 DragControlPoint 风格一致）
                                let mut ghost_builder = path::Builder::new();
                                ghost_builder.move_to(Point::new(start_x, start_y));
                                let steps = 32;
                                for i in 1..=steps {
                                    let t = i as f32 / steps as f32;
                                    // CurveDraw 创建的是 linear_curve()，interpolate(t) = t
                                    let nx = start_x + (cur_x - start_x) * t;
                                    let ny = start_y + (cur_y - start_y) * t;
                                    ghost_builder.line_to(Point::new(nx, ny));
                                }
                                frame.stroke(
                                    &ghost_builder.build(),
                                    canvas::Stroke::default()
                                        .with_color(ghost_alpha)
                                        .with_width(
                                            self.editor.velocity_panel.automation_line_thickness,
                                        ),
                                );

                                // 起点圆点
                                frame.fill(
                                    &path::Path::circle(Point::new(start_x, start_y), 4.0),
                                    ghost_color,
                                );
                                // 终点圆点
                                frame.fill(
                                    &path::Path::circle(Point::new(cur_x, cur_y), 4.0),
                                    ghost_alpha,
                                );
                            }
                        }
                        AutomationDrag::DragControlPoint {
                            prev_tick, which, ..
                        } => {
                            // 控制点拖拽 ghost：绘制更新后的贝塞尔曲线预览
                            self.draw_drag_ctrl_ghost(
                                &mut frame,
                                state,
                                &view_params,
                                max_val,
                                *prev_tick,
                                *which,
                            );
                        }
                    }
                }

                vec![frame.into_geometry()]
            }
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.resize_dragging {
            return mouse::Interaction::ResizingVertically;
        }

        if let Some(cursor_pos) = cursor.position() {
            let local_y = cursor_pos.y - _bounds.y;
            if (0.0..=RESIZE_HANDLE_HEIGHT).contains(&local_y) {
                return mouse::Interaction::ResizingVertically;
            }
        }

        if state.automation_drag.is_some() || state.curve_active {
            return mouse::Interaction::Crosshair;
        }

        if state.tempo_drag_idx.is_some() {
            return mouse::Interaction::Grabbing;
        }

        if state.drag_point_idx.is_some() {
            mouse::Interaction::ResizingVertically
        } else if state.hover_point_idx.is_some()
            || state.hover_anchor_tick.is_some()
            || state.tempo_hover_idx.is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a> super::super::VelocityCanvas<'a> {
    /// 绘制控制点拖拽的 ghost 预览：更新后的贝塞尔曲线 + 控制点位置标记。
    fn draw_drag_ctrl_ghost(
        &self,
        frame: &mut Frame<Renderer>,
        state: &VelocityCanvasState,
        view_params: &AutomationViewParams,
        max_val: f32,
        prev_tick: u32,
        which: CtrlEnd,
    ) {
        use lumino_core::SegmentShape;
        let Some(lane) = self.current_automation_lane() else {
            return;
        };
        let Some(idx) = lane.events.iter().position(|e| e.tick == prev_tick) else {
            return;
        };
        let (Some(prev_evt), Some(next_evt)) = (lane.events.get(idx), lane.events.get(idx + 1))
        else {
            return;
        };
        let ghost_shape = state
            .drag_ctrl_ghost
            .map(|(_, _, s)| s)
            .unwrap_or(prev_evt.shape);
        let ghost_alpha = Color {
            a: 0.5,
            ..automation_node_color()
        };
        let line_thickness = self.editor.velocity_panel.automation_line_thickness;
        let px0 = view_params.tick_to_x(prev_evt.tick);
        let py0 = view_params.value_to_y(prev_evt.value as f32, max_val);
        let px3 = view_params.tick_to_x(next_evt.tick);
        let py3 = view_params.value_to_y(next_evt.value as f32, max_val);
        // 采样贝塞尔曲线画预览（单条 path，32 步足够平滑）
        let mut ghost_builder = path::Builder::new();
        ghost_builder.move_to(Point::new(px0, py0));
        let steps = 32;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let f = ghost_shape.interpolate(t);
            let nx = px0 + (px3 - px0) * t;
            let ny = py0 + (py3 - py0) * f;
            ghost_builder.line_to(Point::new(nx, ny));
        }
        frame.stroke(
            &ghost_builder.build(),
            canvas::Stroke::default()
                .with_color(ghost_alpha)
                .with_width(line_thickness),
        );
        // 绘制被拖控制点的位置标记
        if let SegmentShape::Curve { x1, y1, x2, y2 } = ghost_shape {
            let (rx, ry, x_off, y_off) = match which {
                CtrlEnd::Out => (px0, py0, x1, y1),
                CtrlEnd::In => (px3, py3, x2, y2),
            };
            let cx = rx + (px3 - px0) * x_off * SegmentShape::SCALE;
            let cy = ry + (py3 - py0) * y_off * SegmentShape::SCALE;
            frame.fill(&path::Path::circle(Point::new(cx, cy), 4.0), ghost_alpha);
        }
    }
}
