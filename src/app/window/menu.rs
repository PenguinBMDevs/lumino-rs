use iced::{Task, Theme};

use crate::app::window;

use super::Message;

#[derive(Debug, Clone, Copy)]
pub enum MenuAction {
    File(FileAction),
    Edit(EditAction),
    View(ViewAction),
    Help(HelpAction),
}

impl std::fmt::Display for MenuAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
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
    Find,
}

#[derive(Debug, Clone, Copy)]
pub enum ViewAction {
    Light,
    Dark,
    /* */
}

#[derive(Debug, Clone, Copy)]
pub enum HelpAction {
    About,
    /* */
}

pub fn handle(event: MenuAction, window: &mut window::Window) -> Task<Message> {
    use MenuAction::*;
    match event {
        File(r) => file(r),
        Edit(r) => edit(r),
        View(r) => view(r, window),
        Help(r) => help(r),
    }
}

fn file(event: FileAction) -> Task<Message> {
    use FileAction::*;
    match event {
        New => (),
        Open => (),
        Save => (),
        Close => (),
        /* */
        ImportMidi => (),
        /* */
        Settings => (),
        /* */
        Exit => {
            return Task::done(Message::Window(window::Event::Traffic(
                window::TrafficAction::Close,
            )));
        }
    }
    Task::none()
}

fn edit(event: EditAction) -> Task<Message> {
    use EditAction::*;
    match event {
        Undo => (),
        Redo => (),
        /* */
        Cut => (),
        Copy => (),
        Paste => (),
        SelectAll => (),
        /* */
        Find => (),
    }
    Task::none()
}

fn view(event: ViewAction, window: &mut window::Window) -> Task<Message> {
    use ViewAction::*;
    /* TODO */
    match event {
        Light => window.theme = Theme::CatppuccinLatte,
        Dark => window.theme = Theme::CatppuccinMocha,
        /* */
    }
    Task::none()
}

fn help(event: HelpAction) -> Task<Message> {
    use HelpAction::*;
    match event {
        About => (),
        /*  */
    }
    Task::none()
}
