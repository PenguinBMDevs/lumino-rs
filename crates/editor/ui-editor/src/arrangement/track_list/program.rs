//! 工程走带左侧音轨列表 —— `Program` trait 实现（事件分发 + 绘制入口）
//!
//! 从 `track_list.rs` 抽出，控制文件行数并保持单一职责。
//! 绘制细节在 `track_list/draw.rs`，交互处理在 `track_list/handlers.rs`。

use iced_core::{Point, Rectangle, keyboard};
use iced_widget::canvas::{self, Geometry, Program};

use super::TrackListCanvas;
use super::draw;
use super::state::TrackListState;
use crate::{Message, Renderer, Theme};

impl Program<Message, Theme, Renderer> for TrackListCanvas {
    type State = TrackListState;

    fn update(
        &self,
        state: &mut TrackListState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        puffin::profile_function!();
        self.ensure_state(state);

        match event {
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            canvas::Event::Mouse(iced_core::mouse::Event::WheelScrolled { delta }) => {
                use lumino_ui_core::constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};

                // iced 0.14 事件会分发到整棵 widget 树（无 hover 过滤）：
                // 鼠标在右侧音符区滚轮时本列表也会收到同一事件，必须先确认
                // 鼠标在本 Canvas 范围内，否则 Ctrl+滚轮会与音符区缩放双触发、
                // 普通滚轮会双倍滚动（与 click_canvas 的位置检查保持一致）。
                let pos = cursor.position()?;
                if !bounds.contains(pos) {
                    return None;
                }

                // Ctrl + 滚轮：垂直缩放（与钢琴卷帘键盘区一致：平滑步进 + 指针锚点）
                if self.ctrl_pressed {
                    let factor = crate::zoom::zoom_factor_from_delta(delta)?;
                    return Some(canvas::Action::publish(Message::ArrangementZoomY {
                        zoom: self.zoom_y * factor,
                        fixed_ratio: crate::zoom::fixed_ratio_from_viewport(
                            pos.y - bounds.y,
                            0.0,
                            bounds.height,
                        ),
                    }));
                }

                let (_, dy) = match delta {
                    iced_core::mouse::ScrollDelta::Lines { x, y } => {
                        (x * SCROLL_LINES_SCALE, y * SCROLL_LINES_SCALE)
                    }
                    iced_core::mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                let dy = dy.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
                Some(canvas::Action::publish(Message::ArrangementScrollY(
                    self.scroll_y - dy,
                )))
            }
            canvas::Event::Mouse(iced_core::mouse::Event::ButtonPressed(
                iced_core::mouse::Button::Left,
            )) => {
                // 同 WheelScrolled：iced 全树分发事件，鼠标不在列表范围内时
                // 不得执行选择/拖拽逻辑（否则点击音符区会误选中列表音轨）。
                if let Some(pos) = cursor.position()
                    && bounds.contains(pos)
                {
                    let local_pos = Point::new(pos.x - bounds.x, pos.y - bounds.y);
                    self.handle_left_press(state, local_pos, bounds.width)
                } else {
                    None
                }
            }
            canvas::Event::Mouse(iced_core::mouse::Event::ButtonReleased(
                iced_core::mouse::Button::Left,
            )) => {
                if let Some(pos) = cursor.position() {
                    let local_pos = Point::new(pos.x - bounds.x, pos.y - bounds.y);
                    self.handle_left_release(state, local_pos)
                } else {
                    state.take_drag();
                    None
                }
            }
            canvas::Event::Mouse(iced_core::mouse::Event::CursorMoved { position }) => {
                let local_pos = Point::new(position.x - bounds.x, position.y - bounds.y);
                self.handle_cursor_moved(state, local_pos)
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &TrackListState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        puffin::profile_function!();
        draw::draw(self, state, renderer, theme, bounds)
    }
}
