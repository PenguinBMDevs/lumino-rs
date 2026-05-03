//! Program trait 实现

use super::state::CanvasState;
use super::{keyboard, playback_indicator, remote_cursors, ruler, selection_box};
use crate::editor::{EditState, HitType};
use crate::toolbar::Tool;
use crate::{Message, Renderer, Theme, message::EditorAction};
use iced_core::{Rectangle, mouse};
use iced_widget::canvas::{Action, Event, Geometry, Program};

impl Program<Message, Theme, Renderer> for super::PianoRollGrid<'_> {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let bounds_pos = iced_core::Point::new(bounds.x, bounds.y);
        let bounds_size = iced_core::Size::new(bounds.width, bounds.height);

        let canvas = &self.editor.editor_state.canvas;
        let new_size = iced_core::Point::new(bounds.width, bounds.height);
        if canvas.size != new_size || canvas.offset != bounds_pos {
            return Some(Action::publish(crate::Message::CanvasBoundsChanged {
                offset: bounds_pos,
                size: bounds_size,
            }));
        }

        if let Some(position) = cursor.position() {
            let local_pos = iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
            state.position = Some(local_pos);
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position() {
                    let local_pos =
                        iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
                    return self.handle_left_press(state, local_pos);
                }
            }
            Event::Keyboard(iced_core::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift_pressed = modifiers.shift();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let local_pos = iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
                return Some(Action::publish(Message::EditorAction(EditorAction::Moved(
                    local_pos,
                ))));
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                return Some(Action::publish(Message::EditorAction(
                    EditorAction::Released,
                )));
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(position) = cursor.position() {
                    let local_pos =
                        iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
                    if self.editor.is_inside_canvas(local_pos) {
                        return self.handle_wheel_scroll(delta);
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.editor.current_tool() == Tool::Eraser {
            return mouse::Interaction::Crosshair;
        }

        let interaction = &self.editor.editor_state.interaction;
        match interaction.edit_state {
            EditState::Dragging { .. } => mouse::Interaction::Grabbing,
            EditState::PendingDrag { .. } => mouse::Interaction::Pointer,
            EditState::ResizingStart { .. } | EditState::ResizingEnd { .. } => {
                mouse::Interaction::ResizingHorizontally
            }
            EditState::Drawing { .. } => mouse::Interaction::Crosshair,
            EditState::Selecting { .. } => mouse::Interaction::Crosshair,
            EditState::Scrubbing => mouse::Interaction::Grabbing,
            EditState::Idle => match interaction.hover_state {
                Some((_, HitType::Start)) | Some((_, HitType::End)) => {
                    mouse::Interaction::ResizingHorizontally
                }
                Some((_, HitType::Middle)) => mouse::Interaction::Pointer,
                None => mouse::Interaction::default(),
            },
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        puffin::profile_scope!("grid_widget_draw");
        let mut geometries = Vec::new();

        let keyboard_geom = self
            .editor
            .keyboard_cache
            .draw(renderer, bounds.size(), |frame| {
                keyboard::draw(self.editor, frame, bounds, theme);
            });
        geometries.push(keyboard_geom);

        let ruler_geom = self
            .editor
            .ruler_cache
            .draw(renderer, bounds.size(), |frame| {
                ruler::draw(self.editor, frame, bounds, theme);
            });
        geometries.push(ruler_geom);

        if let Some(selection_geom) = selection_box::draw(self.editor, renderer, theme, bounds) {
            geometries.push(selection_geom);
        }

        let remote_cursor_geometries = remote_cursors::draw(self.editor, renderer, bounds);
        geometries.extend(remote_cursor_geometries);

        let playback_indicator_geom = playback_indicator::draw(self.editor, renderer, bounds);
        geometries.push(playback_indicator_geom);

        geometries
    }
}
