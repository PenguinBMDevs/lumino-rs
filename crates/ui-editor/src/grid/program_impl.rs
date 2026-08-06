//! Program trait 实现

use super::state::GridInteractionState;
use super::{keyboard, playback_indicator, remote_cursors, ruler, selection_box};
use crate::{EditState, HitType};
use crate::{Message, Renderer, Theme, message::EditorAction};
use iced_core::{Rectangle, mouse};
use iced_widget::canvas::{Action, Event, Geometry, Program};
use lumino_message::Tool;

impl Program<Message, Theme, Renderer> for super::PianoRollGrid<'_> {
    type State = GridInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        puffin::profile_function!();

        let bounds_pos = iced_core::Point::new(bounds.x, bounds.y);
        let bounds_size = iced_core::Size::new(bounds.width, bounds.height);

        let canvas = &self.editor.editor_state.canvas;
        let new_size = iced_core::Point::new(bounds.width, bounds.height);
        if canvas.size_x != new_size.x
            || canvas.size_y != new_size.y
            || canvas.offset_x != bounds_pos.x
            || canvas.offset_y != bounds_pos.y
        {
            return Some(Action::publish(
                lumino_ui_core::Message::CanvasBoundsChanged {
                    offset: lumino_ui_core::message::Point2::new(bounds_pos.x, bounds_pos.y),
                    size: lumino_ui_core::message::Size2::new(
                        bounds_size.width,
                        bounds_size.height,
                    ),
                },
            ));
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
                    // 图片转 MIDI 放置模式：优先响应悬浮 √× 按钮
                    if self.editor.editor_state.image_to_midi.mode
                        == lumino_editor_state::ImageToMidiMode::Placing
                        && let Some(btns) = crate::grid::i2m_box::i2m_button_rects(self.editor)
                    {
                        if btns.confirm.contains(local_pos) {
                            return Some(Action::publish(Message::RightSidebar(
                                lumino_message::RightSidebarAction::PlacementConfirm,
                            )));
                        }
                        if btns.cancel.contains(local_pos) {
                            return Some(Action::publish(Message::RightSidebar(
                                lumino_message::RightSidebarAction::PlacementCancel,
                            )));
                        }
                    }
                    return self.handle_left_press(state, local_pos);
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(position) = cursor_over_bounds {
                    let local_pos =
                        iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);
                    // 仅在有效钢琴卷帘区域内打开右键菜单
                    if self.editor.is_inside_canvas(local_pos) {
                        return Some(Action::publish(Message::PianoRollContextMenu(
                            lumino_message::PianoRollContextMenuAction::Open {
                                position: lumino_message::Point2::new(local_pos.x, local_pos.y),
                            },
                        )));
                    }
                }
            }
            Event::Keyboard(iced_core::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift_pressed = modifiers.shift();
                state.control_pressed = modifiers.control();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let local_pos = iced_core::Point::new(position.x - bounds.x, position.y - bounds.y);

                // 更新框选框平滑动画
                self.update_selection_box_animation(Some(local_pos));

                if state.is_loop_dragging {
                    return Some(Action::publish(Message::LoopRange(
                        lumino_ui_core::message::LoopRangeAction::RulerMoved {
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
                        lumino_ui_core::message::Point2::new(local_pos.x, local_pos.y),
                    ))));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                // 清除框选框动画状态
                self.editor.selection_box_anim.set(None);

                if state.is_loop_dragging {
                    state.is_loop_dragging = false;
                    return Some(Action::publish(Message::LoopRange(
                        lumino_ui_core::message::LoopRangeAction::RulerReleased,
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
                    let view = &self.editor.editor_state.view;
                    // Ctrl 状态双通道合并：
                    // - state.control_pressed：iced canvas 内 ModifiersChanged 事件（可能因焦点不送达）
                    // - self.editor.ctrl_pressed()：host 窗口级 CtrlKeyChanged（可靠通道）
                    // 两者兜底，保证 Ctrl+滚轮缩放语义稳定。
                    let ctrl_pressed = state.control_pressed || self.editor.ctrl_pressed();
                    // 标尺区域（顶部小节号栏）：Ctrl+滚轮缩放 X 轴，普通滚轮水平平移
                    if local_pos.y < view.ruler_height {
                        return self.handle_ruler_wheel_scroll(delta, ctrl_pressed, local_pos);
                    }
                    // 键盘区域（左侧琴键栏）：Ctrl+滚轮缩放 Y 轴
                    if local_pos.x < view.keyboard_width {
                        return self.handle_keyboard_wheel_scroll(delta, ctrl_pressed, local_pos);
                    }
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
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        puffin::profile_function!();

        // 图片转 MIDI 放置模式：光标反馈
        let i2m = &self.editor.editor_state.image_to_midi;
        if i2m.is_active() {
            use lumino_editor_state::I2mInteraction;
            return match i2m.interaction {
                I2mInteraction::Selecting => mouse::Interaction::Crosshair,
                I2mInteraction::Dragging => mouse::Interaction::Grabbing,
                I2mInteraction::StretchLeft | I2mInteraction::StretchRight => {
                    mouse::Interaction::ResizingHorizontally
                }
                I2mInteraction::None => {
                    if let Some(pos) = cursor.position() {
                        let local_pos = iced_core::Point::new(pos.x - bounds.x, pos.y - bounds.y);
                        if let Some(hit) = self.editor.hit_test_i2m_region(local_pos) {
                            return match hit {
                                crate::SelectionHitType::LeftEdge
                                | crate::SelectionHitType::RightEdge => {
                                    mouse::Interaction::ResizingHorizontally
                                }
                                crate::SelectionHitType::Inside => mouse::Interaction::Pointer,
                            };
                        }
                    }
                    mouse::Interaction::Crosshair
                }
            };
        }

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
                {
                    puffin::profile_scope!("loop_range_hit_test");
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
                                crate::grid::LoopHitTest::StartHandle
                                | crate::grid::LoopHitTest::EndHandle => {
                                    return mouse::Interaction::ResizingHorizontally;
                                }
                                crate::grid::LoopHitTest::Body => {
                                    return mouse::Interaction::Pointer;
                                }
                                crate::grid::LoopHitTest::None => {}
                            }
                        }
                    }
                }

                // 先检查是否悬停在选择框上
                {
                    puffin::profile_scope!("selection_box_hit_test");
                    if let Some(cursor_pos) = cursor.position() {
                        let local_pos =
                            iced_core::Point::new(cursor_pos.x - bounds.x, cursor_pos.y - bounds.y);
                        if let Some(sel_hit) = self.editor.hit_test_selection_box(local_pos) {
                            return match sel_hit {
                                crate::SelectionHitType::LeftEdge
                                | crate::SelectionHitType::RightEdge => {
                                    mouse::Interaction::ResizingHorizontally
                                }
                                crate::SelectionHitType::Inside => mouse::Interaction::Pointer,
                            };
                        }
                    }
                }

                // 固定指示线模式下：检测是否悬停在指示线上
                {
                    puffin::profile_scope!("playback_indicator_hit_test");
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
        crate::puffin_profiler::grid_widget_draw();
        let mut geometries = Vec::new();

        {
            puffin::profile_scope!("draw::keyboard");
            let keyboard_geom = self
                .editor
                .keyboard_cache
                .draw(renderer, bounds.size(), |frame| {
                    keyboard::draw(self.editor, frame, bounds, theme);
                });
            geometries.push(keyboard_geom);
        }

        // 洋葱皮颜色覆盖层（不使用缓存，每帧独立绘制）
        {
            puffin::profile_scope!("draw::onion_overlay");
            if let Some(onion_geom) = keyboard::draw_onion_overlay(self.editor, renderer, bounds) {
                geometries.push(onion_geom);
            }
        }

        {
            puffin::profile_scope!("draw::ruler");
            let ruler_geom = self
                .editor
                .ruler_cache
                .draw(renderer, bounds.size(), |frame| {
                    ruler::draw(self.editor, frame, bounds, theme);
                });
            geometries.push(ruler_geom);
        }

        {
            puffin::profile_scope!("draw::selection_box");
            if let Some(selection_geom) = selection_box::draw(self.editor, renderer, theme, bounds)
            {
                geometries.push(selection_geom);
            }
        }

        {
            puffin::profile_scope!("draw::i2m_box");
            if let Some(i2m_geom) = crate::grid::i2m_box::draw(self.editor, renderer, theme, bounds)
            {
                geometries.push(i2m_geom);
            }
        }

        {
            puffin::profile_scope!("draw::remote_cursors");
            let remote_cursor_geometries = remote_cursors::draw(self.editor, renderer, bounds);
            geometries.extend(remote_cursor_geometries);
        }

        {
            puffin::profile_scope!("draw::playback_indicator");
            let playback_indicator_geom = playback_indicator::draw(self.editor, renderer, bounds);
            geometries.push(playback_indicator_geom);
        }

        geometries
    }
}
