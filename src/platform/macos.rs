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
}

fn app_menu() -> muda::Result<Submenu> {
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
}

fn file_menu() -> muda::Result<Submenu> {
    Submenu::with_items("File", true, &[&PMI::close_window(None)])
}

fn edit_menu() -> muda::Result<Submenu> {
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
}

fn view_menu() -> muda::Result<Submenu> {
    Submenu::with_items("View", true, &[])
}

fn help_menu() -> muda::Result<Submenu> {
    Submenu::with_items("Help", true, &[])
}

fn menus() -> muda::Result<[Submenu; 5]> {
    Ok([
        app_menu()?,
        file_menu()?,
        edit_menu()?,
        view_menu()?,
        help_menu()?,
    ])
}

pub fn init() -> muda::Result<()> {
    MENU.with(init_inner)
}

fn init_inner(cell: &OnceLock<AppMenu>) -> muda::Result<()> {
    if cell.get().is_some() {
        return Ok(());
    }

    let menu_items = menus()?;
    let items_refs: Vec<&dyn IsMenuItem> =
        menu_items.iter().map(|r| r as &dyn IsMenuItem).collect();
    let menu = Menu::with_items(&items_refs)?;

    let _ = menu.init_for_nsapp();

    let app_menu = AppMenu {
        menu,
        map: HashMap::new(),
    };

    let _ = cell.set(app_menu);

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        MENU.with(|shell| {
            if let Some(_menu) = shell.get() {
                let _id = event.id();
            } else {
                eprintln!("Warning: MenuEvent handler called but MENU is not initialized.");
            }
        });
    }));

    Ok(())
}
