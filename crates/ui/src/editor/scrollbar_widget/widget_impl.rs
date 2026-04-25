use super::types::{Edge, ScrollbarOrientation, ScrollbarState};
use super::widget::ScrollbarWidget;
use crate::{Element, Message, Renderer, Theme};
use iced_core::Background;
use iced_core::border::Border;
use iced_core::layout;
use iced_core::renderer::{self, Quad};
use iced_core::widget::Tree;
use iced_core::{Event, Length, Rectangle, Renderer as _Renderer, Size, mouse};

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

        if let Event::Mouse(mouse_event) = event {
            match mouse_event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some(position) = cursor.position() {
                        if thumb_bounds.contains(position) {
                            let start_pos = match self.orientation {
                                ScrollbarOrientation::Horizontal => position.x,
                                ScrollbarOrientation::Vertical => position.y,
                            };
                            if let Some(edge) = self.get_edge(position, thumb_bounds) {
                                *state = ScrollbarState::DraggingEdge {
                                    start_pos,
                                    start_zoom: self.zoom,
                                    start_thumb_size: thumb_size,
                                    edge,
                                };
                            } else {
                                let scrollable_size = self.actual_max_scroll(track_size);
                                *state = ScrollbarState::Dragging {
                                    start_pos,
                                    start_scroll: self.scroll.clamp(0.0, scrollable_size),
                                };
                            }
                            shell.request_redraw();
                        } else if bounds.contains(position) {
                            let relative_pos = match self.orientation {
                                ScrollbarOrientation::Horizontal => {
                                    (position.x - bounds.x - 2.0)
                                        / (track_size - thumb_size).max(1.0)
                                }
                                ScrollbarOrientation::Vertical => {
                                    (position.y - bounds.y - 2.0)
                                        / (track_size - thumb_size).max(1.0)
                                }
                            };

                            let actual_max_scroll = self.actual_max_scroll(track_size);

                            let new_scroll = relative_pos.clamp(0.0, 1.0) * actual_max_scroll;
                            shell.publish((self.on_scroll)(new_scroll));
                        }
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if matches!(
                        state,
                        ScrollbarState::Dragging { .. } | ScrollbarState::DraggingEdge { .. }
                    ) {
                        let new_state = if let Some(position) = cursor.position() {
                            self.determine_state_at_position(position, thumb_bounds)
                        } else {
                            ScrollbarState::Idle
                        };
                        *state = new_state;
                        shell.request_redraw();
                    }
                }
                mouse::Event::CursorMoved { .. } => match *state {
                    ScrollbarState::Dragging {
                        start_pos,
                        start_scroll,
                    } => {
                        if let Some(position) = cursor.position() {
                            let current_pos = match self.orientation {
                                ScrollbarOrientation::Horizontal => position.x,
                                ScrollbarOrientation::Vertical => position.y,
                            };
                            let delta = current_pos - start_pos;

                            let actual_max_scroll = self.actual_max_scroll(track_size);

                            let scroll_ratio = delta / (track_size - thumb_size).max(1.0);
                            let new_scroll = (start_scroll + scroll_ratio * actual_max_scroll)
                                .clamp(0.0, actual_max_scroll);
                            shell.publish((self.on_scroll)(new_scroll));
                            shell.request_redraw();
                        }
                    }
                    ScrollbarState::DraggingEdge {
                        start_pos,
                        start_zoom,
                        start_thumb_size,
                        edge,
                    } => {
                        if let Some(position) = cursor.position() {
                            let current_pos = match self.orientation {
                                ScrollbarOrientation::Horizontal => position.x,
                                ScrollbarOrientation::Vertical => position.y,
                            };
                            let delta = current_pos - start_pos;
                            let effective_delta = if edge == Edge::End { delta } else { -delta };

                            let max_delta = track_size - start_thumb_size;
                            let clamped_delta = effective_delta.min(max_delta);

                            let ratio = (1.0 + clamped_delta / start_thumb_size.max(1.0)).max(0.1);
                            let new_zoom = start_zoom / ratio;
                            let fixed_ratio = if edge == Edge::End { 0.0 } else { 1.0 };
                            shell.publish((self.on_zoom)(new_zoom, fixed_ratio));
                            shell.request_redraw();
                        }
                    }
                    _ => {
                        let new_state = if let Some(position) = cursor.position() {
                            self.determine_state_at_position(position, thumb_bounds)
                        } else {
                            ScrollbarState::Idle
                        };

                        if *state != new_state {
                            *state = new_state;
                            shell.request_redraw();
                        }
                    }
                },
                _ => {}
            }
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
