//! Program trait 实现 — 事件处理与绘制
//!
//! 按职责拆分为以下子模块：
//! - `mouse_interaction`: 鼠标交互反馈（光标形态）
//! - `draw`: 各图层绘制
//!
//! 输入语义统一在 [`super::canvas_input::dispatch`]：本文件的 `update` 只负责
//! 把 iced 事件翻译成 [`CanvasEvent`]（含画布坐标换算），再转调共享分派器。

mod draw;
pub mod mouse_interaction;

use super::canvas_input::{CanvasButton, CanvasEvent, dispatch};
use super::state::GridInteractionState;
use crate::{Message, Renderer, Theme};
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

        // `cursor_over_bounds` 决定按下/滚轮是否落在画布内；移动与释放不受限
        // （拖拽移出画布后仍需继续跟手）。
        let over = cursor.position_over(bounds);
        let local = |position: iced_core::Point| (position.x - bounds.x, position.y - bounds.y);

        let mapped = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                over.map(|position| CanvasEvent::Press {
                    local: local(position),
                    button: CanvasButton::Left,
                })
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                over.map(|position| CanvasEvent::Press {
                    local: local(position),
                    button: CanvasButton::Right,
                })
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(CanvasEvent::Release)
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => Some(CanvasEvent::Move {
                local: local(*position),
            }),
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => over.map(|position| {
                let (delta_x, delta_y, precise) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (*x, *y, false),
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y, true),
                };
                CanvasEvent::Wheel {
                    local: local(position),
                    delta_x,
                    delta_y,
                    precise,
                }
            }),
            Event::Keyboard(iced_core::keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(CanvasEvent::Modifiers {
                    shift: modifiers.shift(),
                    ctrl: modifiers.control(),
                })
            }
            Event::Window(iced_core::window::Event::Unfocused) => Some(CanvasEvent::FocusLost),
            _ => None,
        };

        mapped
            .and_then(|event| dispatch(self.editor, state, event))
            .map(Action::publish)
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
