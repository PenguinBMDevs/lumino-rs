use iced_core::Length;
use iced_widget::{column, container, progress_bar, row, text};

use crate::{editor, message, sidebar, statusbar, titlebar, window};

pub type Message = message::Message;
pub type Theme = iced_core::Theme;
pub type Renderer = iced_wgpu::Renderer;
pub type Element<'a> = iced_core::Element<'a, Message, Theme, Renderer>;

pub struct Root {
    sidebar: sidebar::Sidebar,
    titlebar: titlebar::Titlebar,
    statusbar: statusbar::StatusBar,
    editor: editor::Editor,
    window: window::Window,
    progress: Option<(String, f64)>,
    is_progress_window: bool,
}

impl Root {
    pub fn new(theme: &str) -> Self {
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
            progress: None,
            is_progress_window: false,
        }
    }

    pub fn new_progress(theme: &str) -> Self {
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
            progress: None,
            is_progress_window: true,
        }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::Core(r) => lumino_core::event::emit(r),
            Message::Window(r) => self.window.update(r),
            Message::Sidebar(r) => self.sidebar.update(r),
            Message::Progress(p) => self.progress = p,
            Message::ScrollbarScrolled(new_scroll_x) => {
                // 处理水平滚动条滚动
                self.editor.set_scroll_x(new_scroll_x);
            }
            Message::ScrollbarScrolledY(new_scroll_y) => {
                // 处理垂直滚动条滚动
                self.editor.set_scroll_y(new_scroll_y);
            }
            // 显式丢弃它
            Message::Null => (),
        }
    }

    pub fn theme(&self) -> Theme {
        self.window.theme.clone()
    }

    pub fn view(&self) -> Element<'_> {
        if self.is_progress_window {
            // 进度窗口只显示进度
            // 默认显示初始化状态，避免窗口空白
            let (msg, progress) = self
                .progress
                .as_ref()
                .map(|(m, p)| (m.as_str(), *p))
                .unwrap_or(("正在初始化...", 0.0));

            container(
                column![
                    text("处理中...")
                        .size(24)
                        .style(|theme: &Theme| text::Style {
                            color: Some(theme.extended_palette().background.neutral.text),
                        }),
                    text(msg).size(16).style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.neutral.text),
                    }),
                    progress_bar(0.0..=1.0, progress as f32),
                ]
                .spacing(20)
                .align_x(iced_core::Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(30)
            .style(|theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(theme.palette().background)),
                ..Default::default()
            })
            .into()
        } else {
            // 主窗口
            let main_content = column![
                self.titlebar.view(&self.window),
                row![self.sidebar.view(), self.editor.view(Message::ScrollbarScrolled, Message::ScrollbarScrolledY),],
                self.statusbar.view(),
            ];

            container(main_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(theme.palette().background)),
                    ..Default::default()
                })
                .into()
        }
    }
}
