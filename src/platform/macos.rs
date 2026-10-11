#![cfg(target_os = "macos")]

use std::{cell::RefCell, collections::HashMap};

use lumino_extras::i18n::Language;
use lumino_ui::event::{self as event, Event as CoreEvent};
use lumino_ui::titlebar::menu::{MenuItem as UiMenuItem, event_display_name, menus as ui_menus};
use muda::{
    IsMenuItem, Menu, MenuEvent, MenuId, MenuItem as MudaMenuItem, PredefinedMenuItem as PMI,
    Submenu,
};

thread_local! {
    /// 当前已安装的原生菜单。
    ///
    /// UI-018：不用 `OnceLock`——原生菜单是 NSMenu，语言变更后必须**整表重建**
    /// （事件映射表也随语言一起换），否则菜单栏会停在旧语言。
    static MENU: RefCell<Option<AppMenu>> = const { RefCell::new(None) };
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
    let first_time = MENU.with(|cell| cell.borrow().is_none());
    install_menu(lang)?;
    if first_time {
        register_event_handler();
    }
    Ok(())
}

/// 构建并安装原生菜单；**可重复调用**（语言变更后整表重建）。
///
/// 安全性依据（muda 0.17 源码）：`init_for_nsapp` 等价于
/// `NSApplication::setMainMenu(Some(new_menu))`，而 `Menu` 的 `Drop` 只清理 GTK
/// 结构、**不会**调用 `setMainMenu(None)`——因此「先装新菜单、旧菜单随后在赋值时
/// 释放」不会把新菜单一起带走。
fn install_menu(lang: Language) -> muda::Result<()> {
    let mut map = HashMap::new();

    let app = app_menu()?;

    // 原生菜单在启动时静态构建，无法感知 UI 的选中状态，
    // 因此导出素材等条件菜单项保持禁用（false）
    let ui_configs = ui_menus(lang, false);
    // 子菜单标题必须按 `lang` 取显示名：`MenuKind` 的 `Display` 曾硬编码简体中文，
    // 导致英文界面下 macOS 菜单栏永远显示中文（UI-018 缺陷 D-2，已删除该 Display）。
    let file = build_submenu(
        ui_configs[0].kind.display_name(lang),
        &ui_configs[0].items,
        lang,
        &mut map,
    )?;
    let edit = build_submenu(
        ui_configs[1].kind.display_name(lang),
        &ui_configs[1].items,
        lang,
        &mut map,
    )?;
    let view = build_submenu(
        ui_configs[2].kind.display_name(lang),
        &ui_configs[2].items,
        lang,
        &mut map,
    )?;
    let help = build_submenu(
        ui_configs[3].kind.display_name(lang),
        &ui_configs[3].items,
        lang,
        &mut map,
    )?;

    let menu = Menu::with_items(&[&app, &file, &edit, &view, &help])?;
    menu.init_for_nsapp();

    MENU.with(|cell| *cell.borrow_mut() = Some(AppMenu { menu, map }));
    Ok(())
}

/// 注册菜单事件分发（**只注册一次**；处理器每次都从 thread-local 读当前菜单，
/// 因此重建菜单后事件映射自动跟随）。
fn register_event_handler() {
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        MENU.with(|shell| match shell.borrow().as_ref() {
            Some(app_menu) => {
                if let Some(core_event) = app_menu.map.get(event.id()) {
                    event::emit(core_event.clone());
                }
            }
            None => tracing::warn!("MenuEvent handler called but MENU is not initialized."),
        });
    }));
}
