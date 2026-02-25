use crate::{Message, Renderer};
use iced_core::{Point, Rectangle, Theme, mouse};
use iced_widget::canvas::{self, Event, Frame, Geometry, Path, Program, Stroke};

// 滚动条状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollbarState {
    Idle,
    HoverThumb,
    DraggingThumb {
        start_x: f32,
        start_thumb_x: f32,
        bounds_width: f32,
    },
}

// 滚动条
pub struct Scrollbar {
    pub thumb_width: f32,
    pub edge_width: f32,
    pub state: ScrollbarState,
    // 用于存储计算出的新滚动值，供外部读取
    pub new_scroll_x: Option<f32>,
    // 当前滑块位置比例 (0.0 ~ 1.0)
    pub thumb_ratio: f32,
}

impl Scrollbar {
    pub fn new(thumb_width: f32) -> Self {
        Self {
            thumb_width,
            edge_width: 5.0,
            state: ScrollbarState::Idle,
            new_scroll_x: None,
            thumb_ratio: 0.0,
        }
    }

    // 根据滚动位置更新滑块比例
    pub fn update_thumb_from_scroll(&mut self, scroll_x: f32, max_scroll: f32) {
        if max_scroll <= 0.0 {
            self.thumb_ratio = 0.0;
            return;
        }
        self.thumb_ratio = (scroll_x / max_scroll).clamp(0.0, 1.0);
    }

    // 根据滑块比例计算滚动值
    pub fn calculate_scroll_from_ratio(&self, max_scroll: f32) -> f32 {
        self.thumb_ratio * max_scroll
    }

    // 计算实际滑块位置
    pub fn thumb_x(&self, bounds_width: f32) -> f32 {
        let available_width = bounds_width - self.thumb_width;
        if available_width <= 0.0 {
            return 0.0;
        }
        self.thumb_ratio * available_width
    }

    // 鼠标是否在滑块上
    pub fn is_mouse_on_thumb(&self, mouse_x: f32, bounds_width: f32) -> bool {
        let thumb_x = self.thumb_x(bounds_width);
        mouse_x >= thumb_x && mouse_x <= thumb_x + self.thumb_width
    }
}

// 滚动条视图
pub struct ScrollbarView<'a> {
    pub scrollbar: &'a mut Scrollbar,
    pub max_scroll: f32,
}

impl<'a> Program<Message, Theme, Renderer> for ScrollbarView<'a> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let scrollbar = unsafe { &mut *(self.scrollbar as *const _ as *mut Scrollbar) };

        if let Event::Mouse(mouse_event) = event {
            match mouse_event {
                iced_core::mouse::Event::ButtonPressed(iced_core::mouse::Button::Left) => {
                    if let Some(position) = cursor.position() {
                        let local_x = position.x - bounds.x;
                        let local_y = position.y - bounds.y;
                        if local_y >= 0.0
                            && local_y <= bounds.height
                            && scrollbar.is_mouse_on_thumb(local_x, bounds.width)
                        {
                            let thumb_x = scrollbar.thumb_x(bounds.width);
                            scrollbar.state = ScrollbarState::DraggingThumb {
                                start_x: local_x,
                                start_thumb_x: thumb_x,
                                bounds_width: bounds.width,
                            };
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

                        if local_y < 0.0 || local_y > bounds.height {
                            if scrollbar.state != ScrollbarState::Idle {
                                scrollbar.state = ScrollbarState::Idle;
                                scrollbar.new_scroll_x = None;
                                return Some(canvas::Action::request_redraw());
                            }
                        } else {
                            match scrollbar.state {
                                ScrollbarState::DraggingThumb {
                                    start_x,
                                    start_thumb_x,
                                    bounds_width,
                                } => {
                                    let delta_x = local_x - start_x;
                                    let new_thumb_x = start_thumb_x + delta_x;
                                    let available_width = bounds_width - scrollbar.thumb_width;
                                    let clamped_thumb_x = new_thumb_x.clamp(0.0, available_width);

                                    if available_width > 0.0 {
                                        scrollbar.thumb_ratio = clamped_thumb_x / available_width;
                                    }

                                    let new_scroll =
                                        scrollbar.calculate_scroll_from_ratio(self.max_scroll);
                                    scrollbar.new_scroll_x = Some(new_scroll);

                                    return Some(canvas::Action::request_redraw());
                                }
                                _ => {
                                    let new_state =
                                        if scrollbar.is_mouse_on_thumb(local_x, bounds.width) {
                                            ScrollbarState::HoverThumb
                                        } else {
                                            ScrollbarState::Idle
                                        };
                                    if scrollbar.state != new_state {
                                        scrollbar.state = new_state;
                                        return Some(canvas::Action::request_redraw());
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        None
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match self.scrollbar.state {
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
        let mut frame = Frame::new(renderer, bounds.size());
        self.scrollbar.draw(&mut frame, theme, bounds);
        vec![frame.into_geometry()]
    }
}

impl Scrollbar {
    // 绘制滚动条
    pub fn draw(&self, frame: &mut Frame<Renderer>, theme: &Theme, bounds: Rectangle) {
        let palette = theme.extended_palette().background;

        // 轨道
        let track_rect = Rectangle::new(
            Point::new(0.0, 0.0),
            iced_core::Size::new(bounds.width, bounds.height),
        );
        let track_path = Path::rectangle(track_rect.position(), track_rect.size());
        frame.fill(&track_path, palette.weakest.color);

        // 滑块颜色
        let thumb_color = match self.state {
            ScrollbarState::DraggingThumb { .. } => palette.strong.color,
            ScrollbarState::HoverThumb => palette.neutral.color,
            _ => palette.weak.color,
        };

        // 计算实际滑块位置
        let thumb_x = self.thumb_x(bounds.width);

        // 滑块
        let thumb_rect = Rectangle::new(
            Point::new(thumb_x, 2.0),
            iced_core::Size::new(self.thumb_width, bounds.height - 4.0),
        );
        let thumb_path = Path::rectangle(thumb_rect.position(), thumb_rect.size());
        frame.fill(&thumb_path, thumb_color);

        // 边缘线
        let edge_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(palette.strong.color);

        let left_edge_x = thumb_x + self.edge_width;
        let left_line = Path::line(
            Point::new(left_edge_x, 2.0),
            Point::new(left_edge_x, bounds.height - 2.0),
        );
        frame.stroke(&left_line, edge_stroke);

        let right_edge_x = thumb_x + self.thumb_width - self.edge_width;
        let right_line = Path::line(
            Point::new(right_edge_x, 2.0),
            Point::new(right_edge_x, bounds.height - 2.0),
        );
        frame.stroke(&right_line, edge_stroke);
    }
}
