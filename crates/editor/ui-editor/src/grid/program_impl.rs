//! Program trait 实现 — 事件处理与绘制
//!
//! 按职责拆分为以下子模块：
//! - `mouse_interaction`: 鼠标交互反馈（光标形态）
//! - `draw`: 各图层绘制

mod draw;
mod mouse_interaction;

use super::state::GridInteractionState;
use crate::message::EditorAction;
use crate::{Message, Renderer, Theme};
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
                    // 曲线工具直线模式：优先响应悬浮 √× 按钮
                    if self.editor.current_tool() == Tool::Curve
                        && let Some(btns) =
                            crate::grid::line_tool_box::line_button_rects(self.editor)
                    {
                        if btns.confirm.contains(local_pos) {
                            return Some(Action::publish(Message::EditorAction(
                                EditorAction::LineToolConfirm,
                            )));
                        }
                        if btns.cancel.contains(local_pos) {
                            return Some(Action::publish(Message::EditorAction(
                                EditorAction::LineToolCancel,
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
        mouse_interaction::handle(self.editor, state, bounds, cursor)
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        draw::draw(self.editor, renderer, theme, bounds)
    }
}
