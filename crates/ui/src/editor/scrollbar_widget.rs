use crate::{Message, Renderer, Theme, Element};
use iced_core::{Length, Rectangle, Size, mouse, Event, Renderer as _Renderer};
use iced_core::widget::Tree;
use iced_core::layout;
use iced_core::renderer::{self, Quad};
use iced_core::border::Border;
use iced_core::Background;

// 滚动条状态（存储在 Tree 中）
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ScrollbarState {
    #[default]
    Idle,
    Hover,
    Dragging { start_x: f32, start_scroll: f32 },
}

// 滚动条 Widget
pub struct ScrollbarWidget<'a> {
    pub scroll_x: f32,
    pub max_scroll: f32,
    pub on_scroll: Box<dyn Fn(f32) -> Message + 'a>,
}

impl<'a> ScrollbarWidget<'a> {
    pub fn new(
        scroll_x: f32,
        max_scroll: f32,
        _width: f32,
        on_scroll: impl Fn(f32) -> Message + 'a,
    ) -> Self {
        Self {
            scroll_x,
            max_scroll,
            on_scroll: Box::new(on_scroll),
        }
    }

    // 计算滑块的几何信息
    fn thumb_geometry(&self, bounds: Rectangle) -> (f32, f32, Rectangle) {
        let track_width = bounds.width - 4.0;
        let thumb_width = 100.0_f32.min(track_width * 0.3);
        let thumb_x = bounds.x + 2.0 + (self.scroll_x / self.max_scroll) * (track_width - thumb_width);
        
        let thumb_bounds = Rectangle {
            x: thumb_x,
            y: bounds.y + 2.0,
            width: thumb_width,
            height: bounds.height - 4.0,
        };
        
        (track_width, thumb_width, thumb_bounds)
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
        Size {
            width: Length::Fill,
            height: Length::Fixed(20.0),
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
                            *state = ScrollbarState::Dragging {
                                start_x: position.x,
                                start_scroll: self.scroll_x,
                            };
                            shell.request_redraw();
                        } else if bounds.contains(position) {
                            let relative_x = (position.x - bounds.x - 2.0) / (track_width - thumb_width);
                            let new_scroll = relative_x.clamp(0.0, 1.0) * self.max_scroll;
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
                        ScrollbarState::Dragging { start_x, start_scroll } => {
                            if let Some(position) = cursor.position() {
                                let delta_x = position.x - start_x;
                                let scroll_ratio = delta_x / (track_width - thumb_width);
                                let new_scroll = (start_scroll + scroll_ratio * self.max_scroll)
                                    .clamp(0.0, self.max_scroll);
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
