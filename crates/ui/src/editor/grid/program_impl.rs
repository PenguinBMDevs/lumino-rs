//! Program trait 实现

use super::state::GridInteractionState;
use super::{keyboard, playback_indicator, remote_cursors, ruler, selection_box};
use crate::editor::{EditState, HitType};
use crate::toolbar::Tool;
use crate::{Message, Renderer, Theme, message::EditorAction};
use iced_core::{Rectangle, mouse};
use iced_widget::canvas::{Action, Event, Geometry, Program};

impl Program<Message, Theme, Renderer> for super::PianoRollGrid<'_> {
    type State = GridInteractionState;

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

        let cursor_over_bounds = cursor.position_over(bounds);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor_over_bounds {
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

                // 更新框选框平滑动画
                self.update_selection_box_animation(Some(local_pos));

                if state.is_loop_dragging {
                    return Some(Action::publish(Message::LoopRange(
                        crate::message::LoopRangeAction::RulerMoved {
                            x: local_pos.x,
                            y: local_pos.y,
                        },
                    )));
                }
                if state.is_dragging_indicator {
                    return Some(Action::publish(Message::EditorAction(
                        EditorAction::IndicatorDragMove { x: local_pos.x },
                    )));
                }
                if cursor_over_bounds.is_some() {
                    return Some(Action::publish(Message::EditorAction(EditorAction::Moved(
                        local_pos,
                    ))));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // 清除框选框动画状态
                *self.editor.selection_box_anim.borrow_mut() = None;

                if state.is_loop_dragging {
                    state.is_loop_dragging = false;
                    return Some(Action::publish(Message::LoopRange(
                        crate::message::LoopRangeAction::RulerReleased,
                    )));
                }
                if state.is_dragging_indicator {
                    state.is_dragging_indicator = false;
                }
                return Some(Action::publish(Message::EditorAction(
                    EditorAction::Released,
                )));
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(position) = cursor_over_bounds {
                    let local_pos =
                        iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
                    if self.editor.is_inside_canvas(local_pos) {
                        return self.handle_wheel_scroll(delta, state.shift_pressed);
                    }
                }
            }
            _ => {}
        }

        None
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.editor.current_tool() == Tool::Eraser {
            return mouse::Interaction::Crosshair;
        }

        let interaction = &self.editor.editor_state.interaction;
        match interaction.edit_state {
            EditState::Dragging { .. } | EditState::DraggingSelection { .. } => {
                mouse::Interaction::Grabbing
            }
            EditState::PendingDrag { .. } => mouse::Interaction::Pointer,
            EditState::ResizingStart { .. }
            | EditState::ResizingEnd { .. }
            | EditState::ResizingSelectionStart { .. }
            | EditState::ResizingSelectionEnd { .. } => mouse::Interaction::ResizingHorizontally,
            EditState::Drawing { .. } => mouse::Interaction::Crosshair,
            EditState::Selecting { .. } => mouse::Interaction::Crosshair,
            EditState::Scrubbing => mouse::Interaction::Grabbing,
            EditState::Idle => {
                // 先检查是否悬停在循环区域手柄上
                if let Some(local_pos) = state.position {
                    let v = &self.editor.editor_state.view;
                    if local_pos.y < v.ruler_height
                        && local_pos.x >= v.keyboard_width
                        && let Some(loop_range) = self.editor.loop_range.as_ref()
                    {
                        let hit = loop_range.hit_test_at(
                            local_pos.x,
                            v.keyboard_width,
                            v.scroll_x,
                            v.zoom_x,
                        );
                        match hit {
                            crate::editor::grid::LoopHitTest::StartHandle
                            | crate::editor::grid::LoopHitTest::EndHandle => {
                                return mouse::Interaction::ResizingHorizontally;
                            }
                            crate::editor::grid::LoopHitTest::Body => {
                                return mouse::Interaction::Pointer;
                            }
                            crate::editor::grid::LoopHitTest::None => {}
                        }
                    }
                }

                // 先检查是否悬停在选择框上
                if let Some(cursor_pos) = _cursor.position() {
                    let local_pos =
                        iced_core::Point::new(cursor_pos.x - _bounds.x, cursor_pos.y - _bounds.y);
                    if let Some(sel_hit) = self.editor.hit_test_selection_box(local_pos) {
                        return match sel_hit {
                            crate::editor::SelectionHitType::LeftEdge
                            | crate::editor::SelectionHitType::RightEdge => {
                                mouse::Interaction::ResizingHorizontally
                            }
                            crate::editor::SelectionHitType::Inside => mouse::Interaction::Pointer,
                        };
                    }
                }

                // 固定指示线模式下：检测是否悬停在指示线上
                if self.editor.editor_state.auto_scroll.mode
                    == lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft
                    && let Some(local_pos) = state.position
                {
                    let v = &self.editor.editor_state.view;
                    if local_pos.y < v.ruler_height && local_pos.x >= v.keyboard_width {
                        let indicator_screen_x = self
                            .editor
                            .get_playback_indicator_screen_x()
                            .unwrap_or(v.keyboard_width);
                        let hit_margin = 8.0;
                        if (local_pos.x - indicator_screen_x).abs() <= hit_margin {
                            return mouse::Interaction::ResizingHorizontally;
                        }
                    }
                }

                match interaction.hover_state {
                    Some((_, HitType::Start)) | Some((_, HitType::End)) => {
                        mouse::Interaction::ResizingHorizontally
                    }
                    Some((_, HitType::Middle)) => mouse::Interaction::Pointer,
                    None => mouse::Interaction::default(),
                }
            }
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
