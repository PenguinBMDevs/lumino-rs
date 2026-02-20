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

// 滚动条状态（存储在 Tree 中）
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ScrollbarState {
    #[default]
    Idle,
    Hover,
    Dragging { start_pos: f32, start_scroll: f32 },
}

// 滚动条 Widget
pub struct ScrollbarWidget<'a> {
    pub scroll: f32,
    pub max_scroll: f32,
    pub orientation: ScrollbarOrientation,
    pub on_scroll: Box<dyn Fn(f32) -> Message + 'a>,
}

impl<'a> ScrollbarWidget<'a> {
    pub fn new(
        scroll: f32,
        max_scroll: f32,
        orientation: ScrollbarOrientation,
        on_scroll: impl Fn(f32) -> Message + 'a,
    ) -> Self {
        Self {
            scroll,
            max_scroll,
            orientation,
            on_scroll: Box::new(on_scroll),
        }
    }

    pub fn horizontal(
        scroll_x: f32,
        max_scroll: f32,
        on_scroll: impl Fn(f32) -> Message + 'a,
    ) -> Self {
        Self::new(scroll_x, max_scroll, ScrollbarOrientation::Horizontal, on_scroll)
    }

    pub fn vertical(
        scroll_y: f32,
        max_scroll: f32,
        on_scroll: impl Fn(f32) -> Message + 'a,
    ) -> Self {
        Self::new(scroll_y, max_scroll, ScrollbarOrientation::Vertical, on_scroll)
    }

    // 计算滑块的几何信息
    fn thumb_geometry(&self, bounds: Rectangle) -> (f32, f32, Rectangle) {
        match self.orientation {
            ScrollbarOrientation::Horizontal => {
                let track_width = bounds.width - 4.0;
                // thumb 大小基于内容比例：当max_scroll越大，thumb越小
                let thumb_width = if self.max_scroll <= 0.0 {
                    track_width // 没有可滚动内容，thumb填满轨道
                } else {
                    (track_width * track_width / (track_width + self.max_scroll)).max(20.0).min(track_width)
                };
                // 确保 scroll 不超过 max_scroll
                let clamped_scroll = self.scroll.clamp(0.0, self.max_scroll);
                let thumb_x = bounds.x + 2.0 + (clamped_scroll / self.max_scroll.max(1.0)) * (track_width - thumb_width);
                
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
        cursor: mouse::Cursor,
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
            Background::Color(palette.weak.color),
        );

        // 计算滑块位置和宽度
        let (_track_width, _thumb_width, thumb_bounds) = self.thumb_geometry(bounds);

        // 判断鼠标是否在滑块上
        let is_hover = if let Some(position) = cursor.position() {
            thumb_bounds.contains(position)
        } else {
            false
        };

        // 根据状态选择滑块颜色
        let thumb_color = match state {
            ScrollbarState::Dragging { .. } => palette.strong.color,
            _ if is_hover => palette.base.color,
            _ => palette.weak.color,
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
        let (track_width, thumb_width, thumb_bounds) = self.thumb_geometry(bounds);

        if let Event::Mouse(mouse_event) = event {
            match mouse_event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some(position) = cursor.position() {
                        if thumb_bounds.contains(position) {
                            let start_pos = match self.orientation {
                                ScrollbarOrientation::Horizontal => position.x,
                                ScrollbarOrientation::Vertical => position.y,
                            };
                            *state = ScrollbarState::Dragging {
                                start_pos,
                                start_scroll: self.scroll,
                            };
                            shell.request_redraw();
                        } else if bounds.contains(position) {
                            let (track_size, thumb_size) = match self.orientation {
                                ScrollbarOrientation::Horizontal => (track_width, thumb_width),
                                ScrollbarOrientation::Vertical => (track_width, thumb_width), // track_width 和 thumb_width 在垂直情况下实际上是高度
                            };
                            let relative_pos = match self.orientation {
                                ScrollbarOrientation::Horizontal => (position.x - bounds.x - 2.0) / (track_size - thumb_size).max(1.0),
                                ScrollbarOrientation::Vertical => (position.y - bounds.y - 2.0) / (track_size - thumb_size).max(1.0),
                            };
                            
                            let actual_max_scroll = match self.orientation {
                                ScrollbarOrientation::Horizontal => self.max_scroll,
                                ScrollbarOrientation::Vertical => (self.max_scroll - track_size).max(0.0),
                            };
                            
                            let new_scroll = relative_pos.clamp(0.0, 1.0) * actual_max_scroll;
                            shell.publish((self.on_scroll)(new_scroll));
                        }
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if matches!(state, ScrollbarState::Dragging { .. }) {
                        let new_state = if let Some(position) = cursor.position() {
                            if thumb_bounds.contains(position) {
                                ScrollbarState::Hover
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
                                let track_size = match self.orientation {
                                    ScrollbarOrientation::Horizontal => track_width,
                                    ScrollbarOrientation::Vertical => track_width, // 在垂直情况下是高度
                                };
                                let thumb_size = match self.orientation {
                                    ScrollbarOrientation::Horizontal => thumb_width,
                                    ScrollbarOrientation::Vertical => thumb_width, // 在垂直情况下是高度
                                };
                                
                                let actual_max_scroll = match self.orientation {
                                    ScrollbarOrientation::Horizontal => self.max_scroll,
                                    ScrollbarOrientation::Vertical => (self.max_scroll - track_size).max(0.0),
                                };
                                
                                let scroll_ratio = delta / (track_size - thumb_size).max(1.0);
                                let new_scroll = (start_scroll + scroll_ratio * actual_max_scroll)
                                    .clamp(0.0, actual_max_scroll);
                                shell.publish((self.on_scroll)(new_scroll));
                            }
                        }
                        _ => {
                            let is_hover = if let Some(position) = cursor.position() {
                                thumb_bounds.contains(position)
                            } else {
                                false
                            };
                            
                            let new_state = if is_hover {
                                ScrollbarState::Hover
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
