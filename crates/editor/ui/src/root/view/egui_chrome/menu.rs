//! 生产菜单树 → egui 菜单模型，以及「动作序号 → `Message`」的解析。
//!
//! 两侧都按同一遍历顺序编号：主菜单 → 菜单项 → 子菜单项。
//! 序号只用于跨 crate 传递（`lumino-ui-egui` 不依赖 iced），真正的事件仍由
//! 生产 `titlebar::menu::menus()` 提供。

use lumino_extras::i18n::Language;
use lumino_ui_egui::chrome::{MenuBarModel, MenuEntry, MenuModel};

use crate::Message;
use crate::root::Root;
use crate::titlebar::menu::{self, MenuConfig, MenuItem};

/// 生产菜单配置 → egui 菜单树。
pub(super) fn menu_bar_model(configs: &[MenuConfig], language: Language) -> MenuBarModel {
    let mut next = 0;
    MenuBarModel {
        menus: configs
            .iter()
            .map(|config| MenuModel {
                label: config.kind.display_name(language).to_owned(),
                entries: entries(&config.items, language, &mut next),
            })
            .collect(),
    }
}

fn entries(items: &[MenuItem], language: Language, next: &mut usize) -> Vec<MenuEntry> {
    items
        .iter()
        .map(|item| match item {
            MenuItem::Action(event) => {
                let action = *next;
                *next += 1;
                MenuEntry::Action {
                    label: menu::event_display_name(event, language),
                    enabled: true,
                    action,
                }
            }
            MenuItem::ActionDisabled(event) => {
                let action = *next;
                *next += 1;
                MenuEntry::Action {
                    label: menu::event_display_name(event, language),
                    enabled: false,
                    action,
                }
            }
            MenuItem::Separator => MenuEntry::Separator,
            MenuItem::Submenu(children, label) => MenuEntry::Submenu {
                label: label.clone(),
                entries: entries(children, language, next),
            },
        })
        .collect()
}

impl Root {
    /// 「导出为素材」是否可用（与标题栏 iced 菜单使用同一判据）。
    pub(super) fn export_material_enabled(&self) -> bool {
        self.editor.has_active_selection(self.edit_view())
    }

    /// 菜单动作序号 → 生产 `Message`。
    ///
    /// 与 [`menu_bar_model`] 同一遍历顺序；禁用项也占一个序号（但不会被点击）。
    pub(crate) fn menu_action_message(&self, index: usize) -> Option<Message> {
        let language = self.settings.display.language;
        let configs = menu::menus(language, self.export_material_enabled());
        let mut next = 0;
        for config in &configs {
            if let Some(message) = find_action(&config.items, index, &mut next) {
                return Some(message);
            }
        }
        None
    }
}

fn find_action(items: &[MenuItem], target: usize, next: &mut usize) -> Option<Message> {
    for item in items {
        match item {
            MenuItem::Action(event) | MenuItem::ActionDisabled(event) => {
                if *next == target {
                    return Some(Message::Core(event.clone()));
                }
                *next += 1;
            }
            MenuItem::Submenu(children, _) => {
                if let Some(message) = find_action(children, target, next) {
                    return Some(message);
                }
            }
            MenuItem::Separator => {}
        }
    }
    None
}
