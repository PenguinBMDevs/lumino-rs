use crate::{Message, Renderer, Theme, Element};
use iced_core::{Length, Rectangle, Size, mouse, Event, Renderer as _Renderer};
use iced_core::widget::Tree;
use iced_core::layout;
use iced_core::renderer::{self, Quad};
use iced_core::border::Border;
use iced_core::Background;

// 滚动条方向
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollbarOrientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edge {
    Start,
    End,
}

// 滚动条状态（存储在 Tree 中）
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ScrollbarState {
    #[default]
    Idle,
    Hover,
    HoverEdge(Edge),
    Dragging { start_pos: f32, start_scroll: f32 },
    DraggingEdge { start_pos: f32, start_zoom: f32, start_thumb_size: f32, edge: Edge },
}

// 滚动条 Widget
pub struct ScrollbarWidget<'a> {
    pub scroll: f32,
    pub max_scroll: f32,
    pub zoom: f32,
    pub orientation: ScrollbarOrientation,
    pub on_scroll: Box<dyn Fn(f32) -> Message + 'a>,
    pub on_zoom: Box<dyn Fn(f32, f32) -> Message + 'a>,
}

impl<'a> ScrollbarWidget<'a> {
    pub fn new(
        scroll: f32,
        max_scroll: f32,
        zoom: f32,
        orientation: ScrollbarOrientation,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self {
            scroll,
            max_scroll,
            zoom,
            orientation,
            on_scroll: Box::new(on_scroll),
            on_zoom: Box::new(on_zoom),
        }
    }

    pub fn horizontal(
        scroll_x: f32,
        max_scroll: f32,
        zoom_x: f32,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self::new(scroll_x, max_scroll, zoom_x, ScrollbarOrientation::Horizontal, on_scroll, on_zoom)
    }

    pub fn vertical(
        scroll_y: f32,
        max_scroll: f32,
        zoom_y: f32,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self::new(scroll_y, max_scroll, zoom_y, ScrollbarOrientation::Vertical, on_scroll, on_zoom)
    }

    // 计算滑块的几何信息
    fn thumb_geometry(&self, bounds: Rectangle) -> (f32, f32, Rectangle) {
        match self.orientation {
            ScrollbarOrientation::Horizontal => {
                let track_width = bounds.width - 4.0;
                // 注意：这里的 max_scroll 实际上是总宽度，我们需要减去视口宽度（track_width）来得到真正的可滚动范围
                let scrollable_width = (self.max_scroll - track_width).max(0.0);

                // thumb 大小基于内容比例：当max_scroll越大，thumb越小
                let thumb_width = if scrollable_width <= 0.0 {
                    track_width // 没有可滚动内容，thumb填满轨道
                } else {
                    (track_width * track_width / self.max_scroll).max(20.0).min(track_width)
                };

                // 确保 scroll 不超过 scrollable_width
                let clamped_scroll = self.scroll.clamp(0.0, scrollable_width);
                let thumb_x = bounds.x + 2.0 + (clamped_scroll / scrollable_width.max(1.0)) * (track_width - thumb_width);

                let thumb_bounds = Rectangle {
                    x: thumb_x,
                    y: bounds.y + 2.0,
                    width: thumb_width,
                    height: bounds.height - 4.0,
                };

                (track_width, thumb_width, thumb_bounds)
            }
            ScrollbarOrientation::Vertical => {
                let track_height = bounds.height - 4.0;
                // thumb 大小基于内容比例：当max_scroll越大，thumb越小
                // 注意：这里的 max_scroll 实际上是总高度，我们需要减去视口高度（track_height）来得到真正的可滚动范围
                let scrollable_height = (self.max_scroll - track_height).max(0.0);

                let thumb_height = if scrollable_height <= 0.0 {
                    track_height // 没有可滚动内容，thumb填满轨道
                } else {
                    (track_height * track_height / self.max_scroll).max(20.0).min(track_height)
                };

                // 确保 scroll 不超过 scrollable_height
                let clamped_scroll = self.scroll.clamp(0.0, scrollable_height);
                let thumb_y = bounds.y + 2.0 + (clamped_scroll / scrollable_height.max(1.0)) * (track_height - thumb_height);

                let thumb_bounds = Rectangle {
                    x: bounds.x + 2.0,
                    y: thumb_y,
                    width: bounds.width - 4.0,
                    height: thumb_height,
                };

                (track_height, thumb_height, thumb_bounds)
            }
        }
    }

    fn get_edge(&self, position: iced_core::Point, thumb_bounds: Rectangle) -> Option<Edge> {
        let edge_width = 6.0;
        match self.orientation {
            ScrollbarOrientation::Horizontal => {
                if position.x >= thumb_bounds.x && position.x <= thumb_bounds.x + edge_width {
                    Some(Edge::Start)
                } else if position.x >= thumb_bounds.x + thumb_bounds.width - edge_width && position.x <= thumb_bounds.x + thumb_bounds.width {
                    Some(Edge::End)
                } else {
                    None
                }
            }
            ScrollbarOrientation::Vertical => {
                if position.y >= thumb_bounds.y && position.y <= thumb_bounds.y + edge_width {
                    Some(Edge::Start)
                } else if position.y >= thumb_bounds.y + thumb_bounds.height - edge_width && position.y <= thumb_bounds.y + thumb_bounds.height {
                    Some(Edge::End)
                } else {
                    None
                }
            }
        }
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

        // 绘制滚动条轨道
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default(),
                shadow: iced_core::Shadow::default(),
                snap: false,
            },
            Background::Color(palette.strongest.color),
        );

        // 计算滑块位置和宽度
        let (_track_width, _thumb_width, thumb_bounds) = self.thumb_geometry(bounds);

        // 根据状态选择滑块颜色
        let thumb_color = match state {
            ScrollbarState::Dragging { .. } | ScrollbarState::DraggingEdge { .. } => palette.strongest.color,
            ScrollbarState::Hover | ScrollbarState::HoverEdge(_) => palette.strong.color,
            _ => palette.base.color,
        };

        // 绘制滑块
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
                                let scrollable_size = (self.max_scroll - track_size).max(0.0);
                                *state = ScrollbarState::Dragging {
                                    start_pos,
                                    start_scroll: self.scroll.clamp(0.0, scrollable_size),
                                };
                            }
                            shell.request_redraw();
                        } else if bounds.contains(position) {
                            let relative_pos = match self.orientation {
                                ScrollbarOrientation::Horizontal => (position.x - bounds.x - 2.0) / (track_size - thumb_size).max(1.0),
                                ScrollbarOrientation::Vertical => (position.y - bounds.y - 2.0) / (track_size - thumb_size).max(1.0),
                            };

                            let actual_max_scroll = (self.max_scroll - track_size).max(0.0);

                            let new_scroll = relative_pos.clamp(0.0, 1.0) * actual_max_scroll;
                            shell.publish((self.on_scroll)(new_scroll));
                        }
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if matches!(state, ScrollbarState::Dragging { .. } | ScrollbarState::DraggingEdge { .. }) {
                        let new_state = if let Some(position) = cursor.position() {
                            if thumb_bounds.contains(position) {
                                if let Some(edge) = self.get_edge(position, thumb_bounds) {
                                    ScrollbarState::HoverEdge(edge)
                                } else {
                                    ScrollbarState::Hover
                                }
                            } else {
                                ScrollbarState::Idle
                            }
                        } else {
                            ScrollbarState::Idle
                        };
                        *state = new_state;
                        shell.request_redraw();
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    match *state {
                        ScrollbarState::Dragging { start_pos, start_scroll } => {
                            if let Some(position) = cursor.position() {
                                let current_pos = match self.orientation {
                                    ScrollbarOrientation::Horizontal => position.x,
                                    ScrollbarOrientation::Vertical => position.y,
                                };
                                let delta = current_pos - start_pos;

                                let actual_max_scroll = (self.max_scroll - track_size).max(0.0);

                                let scroll_ratio = delta / (track_size - thumb_size).max(1.0);
                                let new_scroll = (start_scroll + scroll_ratio * actual_max_scroll)
                                    .clamp(0.0, actual_max_scroll);
                                shell.publish((self.on_scroll)(new_scroll));
                            }
                        }
                        ScrollbarState::DraggingEdge { start_pos, start_zoom, start_thumb_size, edge } => {
                            if let Some(position) = cursor.position() {
                                let current_pos = match self.orientation {
                                    ScrollbarOrientation::Horizontal => position.x,
                                    ScrollbarOrientation::Vertical => position.y,
                                };
                                let delta = current_pos - start_pos;
                                let effective_delta = if edge == Edge::End { delta } else { -delta };

                                // 限制最大拉伸距离，防止滑块超过轨道大小
                                let max_delta = track_size - start_thumb_size;
                                let clamped_delta = effective_delta.min(max_delta);

                                let ratio = (1.0 + clamped_delta / start_thumb_size.max(1.0)).max(0.1);
                                let new_zoom = start_zoom / ratio;
                                let fixed_ratio = if edge == Edge::End { 0.0 } else { 1.0 };
                                shell.publish((self.on_zoom)(new_zoom, fixed_ratio));
                            }
                        }
                        _ => {
                            let new_state = if let Some(position) = cursor.position() {
                                if thumb_bounds.contains(position) {
                                    if let Some(edge) = self.get_edge(position, thumb_bounds) {
                                        ScrollbarState::HoverEdge(edge)
                                    } else {
                                        ScrollbarState::Hover
                                    }
                                } else {
                                    ScrollbarState::Idle
                                }
                            } else {
                                ScrollbarState::Idle
                            };

                            if *state != new_state {
                                *state = new_state;
                                shell.request_redraw();
                            }
                        }
                    }
                }
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
