use iced::{Element, Length, widget::container};

use crate::app::Message;

pub mod editor;
pub mod preview;
pub mod logs;
pub mod settings;

pub trait Page {
    fn update(&mut self, message: Message) -> bool;
    fn view<'a>(&self) -> Element<'a, Message>;
}

pub struct Pages {
    pub editor: editor::EditorPage,
    pub preview: preview::PreviewPage,
    pub logs: logs::LogsPage,
    pub settings: settings::SettingsPage,
}

impl Pages {
    pub fn new() -> Self {
        Self {
            editor: editor::EditorPage::new(),
            preview: preview::PreviewPage::new(),
            logs: logs::LogsPage::new(),
            settings: settings::SettingsPage::new(),
        }
    }
}

pub fn view<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([0, 8])
        .into()
}
