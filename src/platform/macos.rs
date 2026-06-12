#![cfg(target_os = "macos")]

use std::{collections::HashMap, sync::OnceLock};

use lumino_ui::event::{self as event, Event as CoreEvent};
use lumino_ui::titlebar::menu::{MenuItem as UiMenuItem, menus as ui_menus};
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

/// macOS 原生菜单元素包装，用于保持菜单项的原始顺序
enum MenuElement {
    Action(MudaMenuItem),
    Separator(PMI),
    Sub(Submenu),
}

impl MenuElement {
    fn as_is_menu_item(&self) -> &dyn IsMenuItem {
        match self {
            Self::Action(item) => item,
            Self::Separator(item) => item,
            Self::Sub(item) => item,
        }
    }
}

fn build_submenu(
    label: &str,
    items: &[UiMenuItem],
    map: &mut HashMap<MenuId, CoreEvent>,
) -> muda::Result<Submenu> {
    let mut elements: Vec<MenuElement> = Vec::new();

    for item in items {
        match item {
            UiMenuItem::Action(core_event) => {
                let display_name = core_event.display_name();
                let muda_item = MudaMenuItem::new(display_name, true, None);
                let id = muda_item.id().clone();
                map.insert(id, core_event.clone());
                elements.push(MenuElement::Action(muda_item));
            }
            UiMenuItem::Separator => {
                elements.push(MenuElement::Separator(PMI::separator()));
            }
            UiMenuItem::Submenu(sub_items, sub_label) => {
                let sub = build_submenu(sub_label, sub_items, map)?;
                elements.push(MenuElement::Sub(sub));
            }
        }
    }

    let refs: Vec<&dyn IsMenuItem> = elements.iter().map(|e| e.as_is_menu_item()).collect();
    Submenu::with_items(label, true, &refs)
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
    let file = build_submenu(
        &ui_configs[0].kind.to_string(),
        &ui_configs[0].items,
        &mut map,
    )?;
    let edit = build_submenu(
        &ui_configs[1].kind.to_string(),
        &ui_configs[1].items,
        &mut map,
    )?;
    let view = build_submenu(
        &ui_configs[2].kind.to_string(),
        &ui_configs[2].items,
        &mut map,
    )?;
    let help = build_submenu(
        &ui_configs[3].kind.to_string(),
        &ui_configs[3].items,
        &mut map,
    )?;

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
