use super::types::{Edge, ScrollbarOrientation, ScrollbarState};
use super::widget::ScrollbarWidget;
use crate::{Element, Message, Renderer, Theme};
use iced_core::Background;
use iced_core::border::Border;
use iced_core::layout;
use iced_core::renderer::{self, Quad};
use iced_core::widget::Tree;
use iced_core::{Event, Length, Rectangle, Renderer as _Renderer, Size, mouse};

// ─── Mouse event handlers (extracted for ≤4 nesting) ─────────────────

/// 滚动条计算的几何数据
struct ScrollGeometry {
    bounds: Rectangle,
    thumb_bounds: Rectangle,
    track_size: f32,
    thumb_size: f32,
}

impl<'a> ScrollbarWidget<'a> {
    /// 处理鼠标左键按下
    fn handle_button_pressed(
        &mut self,
        state: &mut ScrollbarState,
        geo: &ScrollGeometry,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        let Some(position) = cursor.position() else {
            return;
        };

        if geo.thumb_bounds.contains(position) {
            let start_pos = match self.orientation {
                ScrollbarOrientation::Horizontal => position.x,
                ScrollbarOrientation::Vertical => position.y,
            };
            if let Some(edge) = self.get_edge(position, geo.thumb_bounds) {
                *state = ScrollbarState::DraggingEdge {
                    start_pos,
                    start_zoom: self.zoom,
                    start_thumb_size: geo.thumb_size,
                    edge,
                };
            } else {
                let scrollable_size = self.actual_max_scroll(geo.track_size);
                *state = ScrollbarState::Dragging {
                    start_pos,
                    start_scroll: self.scroll.clamp(0.0, scrollable_size),
                };
            }
            shell.request_redraw();
        } else if geo.bounds.contains(position) {
            let relative_pos = match self.orientation {
                ScrollbarOrientation::Horizontal => {
                    (position.x - geo.bounds.x - 2.0) / (geo.track_size - geo.thumb_size).max(1.0)
                }
                ScrollbarOrientation::Vertical => {
                    (position.y - geo.bounds.y - 2.0) / (geo.track_size - geo.thumb_size).max(1.0)
                }
            };

            let actual_max_scroll = self.actual_max_scroll(geo.track_size);
            let new_scroll = relative_pos.clamp(0.0, 1.0) * actual_max_scroll;
            shell.publish((self.on_scroll)(new_scroll));
        }
    }

    /// 处理鼠标左键释放
    fn handle_button_released(
        &mut self,
        state: &mut ScrollbarState,
        thumb_bounds: Rectangle,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        if !matches!(
            state,
            ScrollbarState::Dragging { .. } | ScrollbarState::DraggingEdge { .. }
        ) {
            return;
        }

        let new_state = cursor.position().map_or(ScrollbarState::Idle, |pos| {
            self.determine_state_at_position(pos, thumb_bounds)
        });
        *state = new_state;
        shell.request_redraw();
    }

    /// 处理鼠标移动
    fn handle_cursor_moved(
        &mut self,
        state: &mut ScrollbarState,
        geo: &ScrollGeometry,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        match *state {
            ScrollbarState::Dragging {
                start_pos,
                start_scroll,
            } => {
                self.handle_dragging(
                    geo.track_size,
                    geo.thumb_size,
                    start_pos,
                    start_scroll,
                    cursor,
                    shell,
                );
            }
            ScrollbarState::DraggingEdge { .. } => {
                self.handle_dragging_edge(state, geo, cursor, shell);
            }
            _ => {
                let new_state = cursor.position().map_or(ScrollbarState::Idle, |pos| {
                    self.determine_state_at_position(pos, geo.thumb_bounds)
                });

                if *state != new_state {
                    *state = new_state;
                    shell.request_redraw();
                }
            }
        }
    }

    /// 处理滑块拖拽中
    fn handle_dragging(
        &self,
        track_size: f32,
        thumb_size: f32,
        start_pos: f32,
        start_scroll: f32,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        let Some(position) = cursor.position() else {
            return;
        };

        let current_pos = match self.orientation {
            ScrollbarOrientation::Horizontal => position.x,
            ScrollbarOrientation::Vertical => position.y,
        };
        let delta = current_pos - start_pos;

        let actual_max_scroll = self.actual_max_scroll(track_size);
        let scroll_ratio = delta / (track_size - thumb_size).max(1.0);
        let new_scroll =
            (start_scroll + scroll_ratio * actual_max_scroll).clamp(0.0, actual_max_scroll);
        shell.publish((self.on_scroll)(new_scroll));
        shell.request_redraw();
    }

    /// 处理边缘拖拽缩放中
    fn handle_dragging_edge(
        &self,
        state: &mut ScrollbarState,
        geo: &ScrollGeometry,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        let (start_pos, start_zoom, start_thumb_size, edge) = match *state {
            ScrollbarState::DraggingEdge {
                start_pos,
                start_zoom,
                start_thumb_size,
                edge,
            } => (start_pos, start_zoom, start_thumb_size, edge),
            _ => return,
        };
        let Some(position) = cursor.position() else {
            return;
        };

        let current_pos = match self.orientation {
            ScrollbarOrientation::Horizontal => position.x,
            ScrollbarOrientation::Vertical => position.y,
        };
        let delta = current_pos - start_pos;
        let effective_delta = if edge == Edge::End { delta } else { -delta };

        // 动态判断滚动是否已达极限，阻止缩小方向的拖拽（放大方向仍可正常工作）
        // 使用 1px 迟滞防止浮点精度导致的误判
        const LIMIT_HYSTERESIS: f32 = 1.0;
        let actual_max_scroll = self.actual_max_scroll(geo.track_size);
        let can_zoom_out = match edge {
            Edge::Start => self.scroll > LIMIT_HYSTERESIS,
            Edge::End => self.scroll < actual_max_scroll - LIMIT_HYSTERESIS,
        };

        if !can_zoom_out && effective_delta > 0.0 {
            // 滚动已达极限，阻止继续缩小。
            // 更新 start_pos 和 start_zoom，使后续 delta ≈ 0、ratio ≈ 1，
            // 避免 effective_delta 突变导致 new_zoom 跳回 start_zoom 产生抽动。
            // 用户反向拖拽（放大）时也能从当前位置平滑过渡。
            *state = ScrollbarState::DraggingEdge {
                start_pos: current_pos,
                start_zoom: self.zoom,
                start_thumb_size,
                edge,
            };
        } else {
            let max_delta = (geo.track_size - start_thumb_size).max(geo.track_size * 0.5);
            let clamped_delta = effective_delta.clamp(-geo.track_size * 0.9, max_delta);

            let ratio = (1.0 + clamped_delta / start_thumb_size.max(1.0)).max(0.05);
            let new_zoom = start_zoom / ratio;
            let fixed_ratio = if edge == Edge::End { 0.0 } else { 1.0 };
            shell.publish((self.on_zoom)(new_zoom, fixed_ratio));
        }
        shell.request_redraw();
    }
}

impl<'a> iced_core::Widget<Message, Theme, Renderer> for ScrollbarWidget<'a> {
    fn tag(&self) -> iced_core::widget::tree::Tag {
        iced_core::widget::tree::Tag::of::<ScrollbarState>()
    }

    fn state(&self) -> iced_core::widget::tree::State {
        iced_core::widget::tree::State::new(ScrollbarState::default())
    }

    fn size(&self) -> Size<Length> {
        match self.orientation {
            ScrollbarOrientation::Horizontal => Size {
                width: Length::Fill,
                height: Length::Fixed(20.0),
            },
            ScrollbarOrientation::Vertical => Size {
                width: Length::Fixed(20.0),
                height: Length::Fill,
            },
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(self.size().width, self.size().height, Size::new(0.0, 0.0));
        layout::Node::new(size)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: iced_core::Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let palette = theme.extended_palette().background;
        let state = tree.state.downcast_ref::<ScrollbarState>();

        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default(),
                shadow: iced_core::Shadow::default(),
                snap: false,
            },
            Background::Color(palette.strong.color),
        );

        let (_track_width, _thumb_width, thumb_bounds) = self.thumb_geometry(bounds);

        let thumb_color = match state {
            ScrollbarState::Dragging { .. } | ScrollbarState::DraggingEdge { .. } => {
                palette.strong.color
            }
            ScrollbarState::Hover | ScrollbarState::HoverEdge(_) => palette.base.color,
            _ => palette.weak.color,
        };

        renderer.fill_quad(
            Quad {
                bounds: thumb_bounds,
                border: Border::default(),
                shadow: iced_core::Shadow::default(),
                snap: false,
            },
            Background::Color(thumb_color),
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: iced_core::Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<ScrollbarState>();
        let (track_size, thumb_size, thumb_bounds) = self.thumb_geometry(bounds);
        let geo = ScrollGeometry {
            bounds,
            thumb_bounds,
            track_size,
            thumb_size,
        };

        let Event::Mouse(mouse_event) = event else {
            return;
        };

        match mouse_event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                self.handle_button_pressed(state, &geo, cursor, shell);
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                self.handle_button_released(state, thumb_bounds, cursor, shell);
            }
            mouse::Event::CursorMoved { .. } => {
                self.handle_cursor_moved(state, &geo, cursor, shell);
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        _layout: iced_core::Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<ScrollbarState>();
        match state {
            ScrollbarState::Dragging { .. } => mouse::Interaction::Grabbing,
            ScrollbarState::DraggingEdge { .. } => match self.orientation {
                ScrollbarOrientation::Horizontal => mouse::Interaction::ResizingHorizontally,
                ScrollbarOrientation::Vertical => mouse::Interaction::ResizingVertically,
            },
            ScrollbarState::HoverEdge(_) => match self.orientation {
                ScrollbarOrientation::Horizontal => mouse::Interaction::ResizingHorizontally,
                ScrollbarOrientation::Vertical => mouse::Interaction::ResizingVertically,
            },
            ScrollbarState::Hover => mouse::Interaction::Pointer,
            _ => mouse::Interaction::default(),
        }
    }
}

impl<'a> From<ScrollbarWidget<'a>> for Element<'a> {
    fn from(scrollbar: ScrollbarWidget<'a>) -> Self {
        Element::new(scrollbar)
    }
}
