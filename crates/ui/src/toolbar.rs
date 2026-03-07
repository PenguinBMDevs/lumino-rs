use iced_core::{Alignment, Length, Point};
use iced_widget::{button, container, mouse_area, row, space};

use crate::{Element, Message, Theme, resources::icon, window};

/// 工具栏事件
#[derive(Debug, Clone)]
pub enum Event {
    Play,
    Pause,
    Stop,
    SkipBackward,
    SkipForward,
    ToolSelected(Tool),
    /// 开始拖拽调整高度
    ResizeDragStarted(Point),
    /// 拖拽中调整高度
    ResizeDragged(Point),
    /// 结束拖拽调整高度
    ResizeDragEnded,
}

impl Event {
    pub const fn play() -> Message {
        Message::Toolbar(Self::Play)
    }

    pub const fn pause() -> Message {
        Message::Toolbar(Self::Pause)
    }

    pub const fn stop() -> Message {
        Message::Toolbar(Self::Stop)
    }

    pub const fn skip_backward() -> Message {
        Message::Toolbar(Self::SkipBackward)
    }

    pub const fn skip_forward() -> Message {
        Message::Toolbar(Self::SkipForward)
    }

    pub const fn tool_selected(tool: Tool) -> Message {
        Message::Toolbar(Self::ToolSelected(tool))
    }

    pub fn resize_drag_started() -> Message {
        Message::Toolbar(Self::ResizeDragStarted(Point::new(0.0, 0.0)))
    }

    pub fn resize_dragged() -> Message {
        Message::Toolbar(Self::ResizeDragged(Point::new(0.0, 0.0)))
    }

    pub const fn resize_drag_ended() -> Message {
        Message::Toolbar(Self::ResizeDragEnded)
    }
}

/// 工具类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Pointer,
    Pencil,
    Brush,
    Pen,
    Eraser,
    Razor,
}

pub struct Toolbar {
    pub current_tool: Tool,
    pub is_playing: bool,
    /// 工具栏高度（默认 72）
    pub height: f32,
    /// 是否正在拖拽调整高度
    is_resizing: bool,
    /// 拖拽开始时的鼠标 Y 坐标
    resize_start_y: f32,
    /// 拖拽开始时的工具栏高度
    resize_start_height: f32,
}

/// 工具栏默认高度
const DEFAULT_HEIGHT: f32 = 72.0;
/// 最小高度
const MIN_HEIGHT: f32 = 56.0;
/// 最大高度
const MAX_HEIGHT: f32 = 200.0;
/// 拖拽手柄高度
const RESIZE_HANDLE_HEIGHT: f32 = 6.0;

impl Toolbar {
    pub fn new() -> Self {
        Self {
            current_tool: Tool::default(),
            is_playing: false,
            height: DEFAULT_HEIGHT,
            is_resizing: false,
            resize_start_y: 0.0,
            resize_start_height: DEFAULT_HEIGHT,
        }
    }

    pub fn view<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        let palette = window.theme.extended_palette();

        // 计算内容区域高度（总高度减去手柄高度）
        let content_height = self.height - RESIZE_HANDLE_HEIGHT;

        // 播放控制区域 (132px 宽)
        let playback_controls = container(
            row![
                tool_button(icon::SkipBackward, Event::skip_backward(), window),
                space().width(4),
                if self.is_playing {
                    tool_button(icon::Pause, Event::pause(), window)
                } else {
                    tool_button(icon::Play, Event::play(), window)
                },
                space().width(4),
                tool_button(icon::SkipForward, Event::skip_forward(), window),
            ]
            .align_y(Alignment::Center),
        )
        .width(132)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        });

        // 工具选择区域 (285px 宽)
        let tools = container(
            row![
                tool_selector(icon::MousePointer, Tool::Pointer, self.current_tool, window),
                space().width(4),
                tool_selector(icon::Pencil, Tool::Pencil, self.current_tool, window),
                space().width(4),
                tool_selector(icon::Eraser, Tool::Eraser, self.current_tool, window),
            ]
            .align_y(Alignment::Center),
        )
        .width(285)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        });

        // 调整大小手柄区域
        let resize_handle: Element<'_> = mouse_area(
            container(space().height(Length::Fixed(RESIZE_HANDLE_HEIGHT)))
                .width(Length::Fill)
                .style(move |_theme: &Theme| {
                    container::Style::default()
                        .background(if self.is_resizing {
                            palette.primary.strong.color
                        } else {
                            palette.background.weakest.color
                        })
                }),
        )
        .interaction(iced_core::mouse::Interaction::ResizingVertically)
        .on_press(Message::Toolbar(Event::ResizeDragStarted(Point::new(0.0, 0.0))))
        .on_release(Message::Toolbar(Event::ResizeDragEnded))
        .into();

        // 主工具栏内容 - 横向排列所有区域
        let toolbar_content = container(row![playback_controls, space().width(16), tools].align_y(Alignment::Center))
            .width(Length::Fill)
            .height(Length::Fixed(content_height))
            .padding([8, 16])
            .style(move |_theme: &Theme| {
                container::Style::default().background(palette.background.weakest.color)
            });

        // 组合工具栏内容和调整手柄
        iced_widget::column![toolbar_content, resize_handle]
            .width(Length::Fill)
            .height(Length::Fixed(self.height))
            .into()
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Play => self.is_playing = true,
            Event::Pause => self.is_playing = false,
            Event::Stop => self.is_playing = false,
            Event::SkipBackward => {}
            Event::SkipForward => {}
            Event::ToolSelected(tool) => self.current_tool = tool,
            Event::ResizeDragStarted(_) => {
                // 拖拽开始由 Host 处理，这里只需要标记状态
                self.is_resizing = true;
            }
            Event::ResizeDragged(_) => {
                // 拖拽中的位置更新由 Host 通过 update_resize_position 处理
            }
            Event::ResizeDragEnded => {
                self.is_resizing = false;
            }
        }
    }

    /// 检查是否正在调整大小
    pub fn is_resizing(&self) -> bool {
        self.is_resizing
    }

    /// 开始调整大小，记录起始鼠标 Y 坐标
    pub fn start_resize(&mut self, cursor_y: f32) {
        self.is_resizing = true;
        self.resize_start_y = cursor_y;
        self.resize_start_height = self.height;
    }

    /// 更新拖拽位置（从外部传入当前鼠标 Y 坐标）
    pub fn update_resize_position(&mut self, cursor_y: f32) {
        if self.is_resizing {
            let delta_y = cursor_y - self.resize_start_y;
            let new_height = self.resize_start_height + delta_y;
            self.height = new_height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        }
    }

    /// 结束调整大小
    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }

    /// 获取当前高度
    pub fn height(&self) -> f32 {
        self.height
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

/// 工具按钮
fn tool_button<'a>(
    icon_enum: icon::Icon,
    on_press: Message,
    window: &'a window::Window,
) -> Element<'a> {
    button(icon::view_with_size_and_theme(
        icon_enum,
        20,
        20,
        Some(&window.theme),
    ))
    .on_press(on_press)
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(4)
    .into()
}

/// 工具选择器
fn tool_selector<'a>(
    icon_enum: icon::Icon,
    tool: Tool,
    current_tool: Tool,
    window: &'a window::Window,
) -> Element<'a> {
    let is_selected = tool == current_tool;
    let palette = window.theme.extended_palette();

    button(icon::view_with_size_and_theme(
        icon_enum,
        17,
        17,
        Some(&window.theme),
    ))
    .on_press(Event::tool_selected(tool))
    .style(move |_theme: &Theme, status| {
        let bg = if is_selected {
            palette.background.strong.color
        } else if status == iced_widget::button::Status::Hovered {
            palette.background.weak.color
        } else {
            iced_core::Color::TRANSPARENT
        };

        button::Style {
            border: iced_core::Border {
                radius: 3.0.into(),
                width: 0.0,
                color: iced_core::Color::TRANSPARENT,
            },
            ..Default::default()
        }
        .with_background(bg)
    })
    .padding(iced_core::Padding::new(4.0))
    .into()
}
