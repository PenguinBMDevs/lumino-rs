use iced_core::Length;
use iced_widget::{column, container, row};

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
}

impl Root {
    pub fn new(theme: &str) -> Self {
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
        }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::Core(r) => lumino_core::event::emit(r),
            Message::Window(r) => self.window.update(r),
            Message::Sidebar(r) => self.sidebar.update(r),
            // Explictly drop it
            Message::Null => (),
        }
    }

    pub fn theme(&self) -> Theme {
        self.window.theme.clone()
    }

    pub fn view(&self) -> Element<'_> {
        let inner = column![
            self.titlebar.view(&self.window),
            row![self.sidebar.view(), self.editor.view(),],
            self.statusbar.view(),
        ];

        container(inner)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(theme.palette().background)),
                ..Default::default()
            })
            .into()
    }
}
