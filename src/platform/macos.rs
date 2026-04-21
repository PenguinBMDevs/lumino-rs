#![cfg(target_os = "macos")]

use std::{collections::HashMap, sync::OnceLock};

use lumino_core::{Event as CoreEvent, event};
use lumino_ui::titlebar::menu::{MenuConfig, MenuItem as UiMenuItem, menus as ui_menus};
use muda::{
    IsMenuItem, Menu, MenuEvent, MenuId, MenuItem as MudaMenuItem, PredefinedMenuItem as PMI,
    Submenu,
};

thread_local! {
    static MENU: OnceLock<AppMenu> = OnceLock::new();
}

struct AppMenu {
    menu: Menu,
    map: HashMap<MenuId, CoreEvent>,
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

fn build_submenu(
    cfg: &MenuConfig,
    map: &mut HashMap<MenuId, CoreEvent>,
) -> muda::Result<Submenu> {
    let mut muda_items: Vec<MudaMenuItem> = Vec::new();
    let mut separators: Vec<PMI> = Vec::new();

    for item in &cfg.items {
        match item {
            UiMenuItem::Action(core_event) => {
                let label = format!("{:?}", core_event);
                let muda_item = MudaMenuItem::new(label, true, None);
                let id = muda_item.id().clone();
                map.insert(id, core_event.clone());
                muda_items.push(muda_item);
            }
            UiMenuItem::Separator => {
                separators.push(PMI::separator());
            }
            UiMenuItem::Submenu(_, _) => {}
        }
    }

    let mut refs: Vec<&dyn IsMenuItem> = Vec::new();
    for item in &muda_items {
        refs.push(item);
    }
    for sep in &separators {
        refs.push(sep);
    }

    Submenu::with_items(&cfg.kind.to_string(), true, &refs)
}

pub fn init() -> muda::Result<()> {
    MENU.with(init_inner)
}

fn init_inner(cell: &OnceLock<AppMenu>) -> muda::Result<()> {
    if cell.get().is_some() {
        return Ok(());
    }

    let mut map = HashMap::new();

    let app = app_menu()?;

    let ui_configs = ui_menus();
    let file = build_submenu(&ui_configs[0], &mut map)?;
    let edit = build_submenu(&ui_configs[1], &mut map)?;
    let view = build_submenu(&ui_configs[2], &mut map)?;
    let help = build_submenu(&ui_configs[3], &mut map)?;

    let menu = Menu::with_items(&[&app, &file, &edit, &view, &help])?;
    let _ = menu.init_for_nsapp();

    let app_menu = AppMenu { menu, map };
    let _ = cell.set(app_menu);

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        MENU.with(|shell| {
            if let Some(app_menu) = shell.get() {
                if let Some(core_event) = app_menu.map.get(event.id()) {
                    event::emit(core_event.clone());
                }
            } else {
                tracing::warn!("MenuEvent handler called but MENU is not initialized.");
            }
        });
    }));

    Ok(())
}
