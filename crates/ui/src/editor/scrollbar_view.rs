use crate::{Message, Renderer, Theme};
use iced_widget::canvas::{self, Frame, Geometry, Program};
use iced_core::{Rectangle, mouse};
use std::cell::RefCell;

use super::scrollbar::{self, ScrollbarState};

// 滚动条视图
pub struct ScrollbarView<'a> {
    pub scrollbar: &'a RefCell<scrollbar::Scrollbar>,
    pub max_scroll: f32,
    // 频率控制：上次发送消息的 scroll_x 值
    pub last_reported_scroll: RefCell<f32>,
    // 频率控制：最小变化阈值（像素）
    pub report_threshold: f32,
}

impl<'a> Program<Message, Theme, Renderer> for ScrollbarView<'a> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let mut scrollbar = self.scrollbar.borrow_mut();

        match event {
            canvas::Event::Mouse(mouse_event) => {
                match mouse_event {
                    iced_core::mouse::Event::ButtonPressed(iced_core::mouse::Button::Left) => {
                        if let Some(position) = cursor.position() {
                            let local_x = position.x - bounds.x;
                            let local_y = position.y - bounds.y;
                            
                            // 检查鼠标是否在滚动条区域内（X和Y都要检查）
                            if local_y >= 0.0 && local_y <= bounds.height &&
                               scrollbar.is_mouse_on_thumb(local_x, bounds.width) {
                                let thumb_x = scrollbar.thumb_x(bounds.width);
                                scrollbar.state = ScrollbarState::DraggingThumb {
                                    start_x: local_x,
                                    start_thumb_x: thumb_x,
                                    bounds_width: bounds.width,
                                };
                                // 重置上次报告的值
                                *self.last_reported_scroll.borrow_mut() = -9999.0;
                                return Some(canvas::Action::request_redraw());
                            }
                        }
                    }
                    iced_core::mouse::Event::ButtonReleased(iced_core::mouse::Button::Left) => {
                        if scrollbar.state != ScrollbarState::Idle {
                            scrollbar.state = ScrollbarState::Idle;
                            scrollbar.new_scroll_x = None;
                            return Some(canvas::Action::request_redraw());
                        }
                    }
                    iced_core::mouse::Event::CursorMoved { .. } => {
                        if let Some(position) = cursor.position() {
                            let local_x = position.x - bounds.x;
                            let local_y = position.y - bounds.y;

                            match scrollbar.state {
                                ScrollbarState::DraggingThumb { start_x, start_thumb_x, bounds_width } => {
                                    // 拖动途中不检测Y范围，只计算X位置
                                    let delta_x = local_x - start_x;
                                    let new_thumb_x = start_thumb_x + delta_x;

                                    // 限制滑块在轨道内
                                    let available_width = bounds_width - scrollbar.thumb_width;
                                    let clamped_thumb_x = new_thumb_x.max(0.0).min(available_width);

                                    // 更新比例
                                    if available_width > 0.0 {
                                        scrollbar.thumb_ratio = clamped_thumb_x / available_width;
                                    }

                                    // 计算新的滚动位置
                                    let new_scroll = scrollbar.calculate_scroll_from_ratio(self.max_scroll);
                                    scrollbar.new_scroll_x = Some(new_scroll);

                                    // 频率控制：检查变化是否超过阈值
                                    let last_reported = *self.last_reported_scroll.borrow();
                                    if (new_scroll - last_reported).abs() >= self.report_threshold {
                                        *self.last_reported_scroll.borrow_mut() = new_scroll;
                                        // 发送消息通知 Editor 更新 scroll_x
                                        return Some(canvas::Action::publish(Message::ScrollbarScrolled(new_scroll)));
                                    }

                                    return Some(canvas::Action::request_redraw());
                                }
                                _ => {
                                    // 非拖动状态，检测是否在滚动条区域内
                                    if local_y >= 0.0 && local_y <= bounds.height {
                                        // 更新悬停状态
                                        let new_state = if scrollbar.is_mouse_on_thumb(local_x, bounds.width) {
                                            ScrollbarState::HoverThumb
                                        } else {
                                            ScrollbarState::Idle
                                        };
                                        if scrollbar.state != new_state {
                                            scrollbar.state = new_state;
                                            return Some(canvas::Action::request_redraw());
                                        }
                                    } else if scrollbar.state != ScrollbarState::Idle {
                                        // 鼠标离开滚动条区域，重置悬停状态
                                        scrollbar.state = ScrollbarState::Idle;
                                        return Some(canvas::Action::request_redraw());
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
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
        let scrollbar = self.scrollbar.borrow();
        match scrollbar.state {
            ScrollbarState::DraggingThumb { .. } => mouse::Interaction::Grabbing,
            ScrollbarState::HoverThumb => mouse::Interaction::Pointer,
            _ => mouse::Interaction::default(),
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
        let scrollbar = self.scrollbar.borrow();
        let mut frame = Frame::new(renderer, bounds.size());
        scrollbar.draw(&mut frame, theme, bounds);
        vec![frame.into_geometry()]
    }
}
