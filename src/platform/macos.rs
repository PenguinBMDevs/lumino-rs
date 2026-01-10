#![cfg(target_os = "macos")]

use std::{collections::HashMap, sync::OnceLock};

use lumino_core::{Event, event};
use muda::{IsMenuItem, Menu, MenuEvent, MenuId, PredefinedMenuItem as PMI, Submenu};

thread_local! {
    static MENU: OnceLock<AppMenu> = OnceLock::new();
}

struct AppMenu {
    menu: Menu,
    map: HashMap<MenuId, Event>,
    // _submenus: Vec<Submenu>,
}

fn app_menu() -> Submenu {
    Submenu::with_items(
        "App",
        true,
        &[
            &PMI::about(None, None),
            &PMI::separator(),
            &PMI::services(None),
            &PMI::separator(),
            &PMI::hide(None),
            &PMI::hide_others(None),
            &PMI::show_all(None),
            &PMI::separator(),
            &PMI::quit(None),
        ],
    )
    .expect("Build App menu")
}

fn file_menu() -> Submenu {
    Submenu::with_items("File", true, &[&PMI::close_window(None)]).expect("Build File menu")
}

fn edit_menu() -> Submenu {
    Submenu::with_items(
        "Edit",
        true,
        &[
            &PMI::undo(None),
            &PMI::redo(None),
            &PMI::separator(),
            &PMI::cut(None),
            &PMI::copy(None),
            &PMI::paste(None),
            &PMI::select_all(None),
        ],
    )
    .expect("Build Edit menu")
}

fn view_menu() -> Submenu {
    Submenu::with_items("View", true, &[]).expect("Build View menu")
}

fn help_menu() -> Submenu {
    Submenu::with_items("Help", true, &[]).expect("Build Help menu")
}

fn menus() -> [Submenu; 5] {
    [
        app_menu(),
        file_menu(),
        edit_menu(),
        view_menu(),
        help_menu(),
    ]
    // todo!()
}

pub fn init() {
    MENU.with(init_inner);
}

fn init_inner(cell: &OnceLock<AppMenu>) {
    if cell.get().is_some() {
        return;
    }

    let menu = Menu::with_items(
        &menus()
            .iter()
            .map(|r| r as &dyn IsMenuItem)
            .collect::<Vec<_>>(),
    )
    .expect("Build menu");

    menu.init_for_nsapp();

    let app_menu = AppMenu {
        menu,
        map: HashMap::new(),
    };

    let _ = cell.set(app_menu);

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        MENU.with(|shell| {
            let id = event.id();
            let menu = shell.get().expect("Get menu");
            // menu.map.get(id)
            // lumino_core::event::emit();
        });
    }));
}
