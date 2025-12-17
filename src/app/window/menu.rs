use iced::Task;

use super::Message;

#[derive(Debug, Clone, Copy)]
pub enum MenuAction {
    File(FileAction),
    Edit(EditAction),
    View(ViewAction),
    Help(HelpAction),
}

#[derive(Debug, Clone, Copy)]
pub enum FileAction {
    New,
    Open,
    Save,
    Close,
    /* */
    ImportMidi,
    /* */
    Settings,
    /* */
    Exit,
}

#[derive(Debug, Clone, Copy)]
pub enum EditAction {
    Undo,
    Redo,
    /* */
    Cut,
    Copy,
    Paste,
    SelectAll,
    /* */
    Find
}

#[derive(Debug, Clone, Copy)]
pub enum ViewAction {
    /* */
}

#[derive(Debug, Clone, Copy)]
pub enum HelpAction {
    /* */
}

pub fn handle() -> Task<Message> {
    /* TODO */
    Task::none()
}
