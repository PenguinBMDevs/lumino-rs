use iced::{
    Task, Theme, window
};


use crate::app::window::Window;

use super::Message;

#[derive(Debug, Clone, Copy)]
pub enum MenuAction {
    File(FileAction),
    Edit(EditAction),
    View(ViewAction),
    Help(HelpAction),
}

impl ToString for MenuAction {
    fn to_string(&self) -> String {
        format!("{self:?}")
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
    Find
}

#[derive(Debug, Clone, Copy)]
pub enum ViewAction {
    Light,
    Dark
    /* */
}

#[derive(Debug, Clone, Copy)]
pub enum HelpAction {
    About,
    /* */
}

pub fn handle(id: window::Id, event: MenuAction, window: &mut Window) -> Task<Message> {
    use MenuAction::*;
    match event {
        File(r) => file(id, r),
        Edit(r) => edit(id, r),
        View(r) => view(id, r, window),
        Help(r) => help(id, r),
    }
}

fn file(id: window::Id, event: FileAction) -> Task<Message> {
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
        Exit => return window::close(id),
    }
    Task::none()
}

fn edit(id: window::Id, event: EditAction) -> Task<Message> {
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

fn view(id: window::Id, event: ViewAction, window: &mut Window) -> Task<Message> {
    use ViewAction::*;
    /* TODO */
    match event {
        Light => window.theme = Theme::CatppuccinLatte,
        Dark => window.theme = Theme::CatppuccinMocha,
        /* */
    }
    Task::none()
}

fn help(id: window::Id, event: HelpAction) -> Task<Message> {
    use HelpAction::*;
    match event {
        About => (),
        /*  */
    }
    Task::none()
}
