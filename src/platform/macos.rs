#![cfg(target_os = "macos")]

use std::{collections::HashMap, sync::OnceLock};

use lumino_extras::i18n::Language;
use lumino_ui::event::{self as event, Event as CoreEvent};
use lumino_ui::titlebar::menu::{MenuItem as UiMenuItem, event_display_name, menus as ui_menus};
use muda::{
    IsMenuItem, Menu, MenuEvent, MenuId, MenuItem as MudaMenuItem, PredefinedMenuItem as PMI,
    Submenu,
};

thread_local! {
    static MENU: OnceLock<AppMenu> = const { OnceLock::new() };
}

struct AppMenu {
    // 持有 Menu 以维持 NSMenu 生命周期；clippy 会误判为未读字段。
    #[allow(dead_code)]
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
    lang: Language,
    map: &mut HashMap<MenuId, CoreEvent>,
) -> muda::Result<Submenu> {
    let mut elements: Vec<MenuElement> = Vec::new();

    for item in items {
        match item {
            UiMenuItem::Action(core_event) => {
                let display_name = event_display_name(core_event, lang);
                let muda_item = MudaMenuItem::new(display_name, true, None);
                let id = muda_item.id().clone();
                map.insert(id, core_event.clone());
                elements.push(MenuElement::Action(muda_item));
            }
            UiMenuItem::ActionDisabled(core_event) => {
                // 禁用项：仅展示显示名，不可点击，也不注册事件映射
                let display_name = event_display_name(core_event, lang);
                let muda_item = MudaMenuItem::new(display_name, false, None);
                elements.push(MenuElement::Action(muda_item));
            }
            UiMenuItem::Separator => {
                elements.push(MenuElement::Separator(PMI::separator()));
            }
            UiMenuItem::Submenu(sub_items, sub_label) => {
                let sub = build_submenu(sub_label, sub_items, lang, map)?;
                elements.push(MenuElement::Sub(sub));
            }
        }
    }

    let refs: Vec<&dyn IsMenuItem> = elements.iter().map(|e| e.as_is_menu_item()).collect();
    Submenu::with_items(label, true, &refs)
}

pub fn init(lang: Language) -> muda::Result<()> {
    MENU.with(|cell| init_inner(lang, cell))
}

fn init_inner(lang: Language, cell: &OnceLock<AppMenu>) -> muda::Result<()> {
    if cell.get().is_some() {
        return Ok(());
    }

    let mut map = HashMap::new();

    let app = app_menu()?;

    // 原生菜单在启动时静态构建，无法感知 UI 的选中状态，
    // 因此导出素材等条件菜单项保持禁用（false）
    let ui_configs = ui_menus(lang, false);
    let file = build_submenu(
        &ui_configs[0].kind.to_string(),
        &ui_configs[0].items,
        lang,
        &mut map,
    )?;
    let edit = build_submenu(
        &ui_configs[1].kind.to_string(),
        &ui_configs[1].items,
        lang,
        &mut map,
    )?;
    let view = build_submenu(
        &ui_configs[2].kind.to_string(),
        &ui_configs[2].items,
        lang,
        &mut map,
    )?;
    let help = build_submenu(
        &ui_configs[3].kind.to_string(),
        &ui_configs[3].items,
        lang,
        &mut map,
    )?;

    let menu = Menu::with_items(&[&app, &file, &edit, &view, &help])?;
    menu.init_for_nsapp();

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
