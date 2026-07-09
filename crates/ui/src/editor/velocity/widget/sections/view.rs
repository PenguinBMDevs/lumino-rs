//! 视图绘制方法：Canvas Program 实现（事件分发、绘制、鼠标交互反馈）

use iced_core::{Point, Rectangle, keyboard, mouse};
use iced_wgpu::Geometry as Geom;
use iced_widget::canvas::{self, Frame, Program};

use crate::{Message, Renderer, Theme};

use super::super::super::{EditMode, PANEL_PADDING_X, RESIZE_HANDLE_HEIGHT, VelocityPanel};
use super::super::drawing::{
    draw_horizontal_lines, draw_scale_labels, draw_tempo_graph, draw_vertical_lines,
};
use super::super::state::VelocityCanvasState;

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

        let cursor_pos = match cursor.position() {
            Some(pos) => Point::new(pos.x - bounds.x, pos.y - bounds.y),
            None => return None,
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
                self.handle_button_released(state)
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
        _state: &Self::State,
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
                    draw_tempo_graph(&mut frame, theme, &tempo_points, bounds.size(), view);
                }
                vec![frame.into_geometry()]
            }
            _ => {
                let mut frame = Frame::new(renderer, bounds.size());
                let view = &self.editor.editor_state.view;
                draw_vertical_lines(&mut frame, theme, bounds.size(), view);
                draw_horizontal_lines(&mut frame, theme, bounds.size(), self.edit_mode);
                draw_scale_labels(&mut frame, theme, bounds.size(), self.edit_mode);
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

        if state.drag_point_idx.is_some() {
            mouse::Interaction::ResizingVertically
        } else if state.hover_point_idx.is_some() || state.hover_anchor_tick.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}
