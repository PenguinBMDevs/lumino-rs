#![cfg(target_os = "macos")]

use std::{cell::OnceCell, collections::HashMap};
use muda::{IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use crate::{
    app::{
        Message, window::{
            self,
            menu::MenuAction,
        }, worker
    },
    ui::window::{
        MenuItem as UiMenuItem, menus
    }
};

thread_local! {
    static MENU: OnceCell<AppMenu> = OnceCell::new();
}

struct AppMenu {
    menu: Menu,
    map: HashMap<MenuId, MenuAction>,
    _submenus: Vec<Submenu>,
    _items: Vec<MenuItem>,
    _seps: Vec<PredefinedMenuItem>,
}

pub fn init() {
    MENU.with(|cell| {
        if cell.get().is_some() {
            return;
        }

        let menu = Menu::new();
        let mut map = HashMap::new();
        let mut _submenus = Vec::new();
        let mut _items = Vec::new();
        let mut _seps = Vec::new();

        let app_menu = Submenu::new("App", true);
        app_menu.append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ]).unwrap();
        menu.append(&app_menu).unwrap();
        _submenus.push(app_menu);

        for cfg in menus() {
            let mut items = Vec::new();
            let mut seps = Vec::new();

            for entry in cfg.items {
                match entry {
                    UiMenuItem::Action(r) => {
                        let item = MenuItem::new(
                            r.to_string(),
                            true,
                            None
                        );
                        map.insert(item.id().clone(), r);
                        items.push(item);
                    },
                    UiMenuItem::Separator => {
                        let item = PredefinedMenuItem::separator();
                        seps.push(item);
                    },
                }
            }
            let mut refs: Vec<&dyn IsMenuItem> = Vec::new();
            refs.extend(items.iter().map(|i| i as &dyn IsMenuItem));
            refs.extend(seps.iter().map(|s| s as &dyn IsMenuItem));
            let submenu = Submenu::with_items(
                cfg.kind.to_string(),
                true,
                &refs
            )
                .expect("Failed to build submenu");

            menu.append(&submenu)
                .expect("Failed to build menu");

            _items.extend(items);
            _seps.extend(seps);
            _submenus.push(submenu);
        }

        let app_menu = AppMenu {
            menu,
            map,
            _submenus,
            _items,
            _seps
        };

        app_menu.menu.init_for_nsapp();

        let _ = cell.set(app_menu);

        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            MENU.with(|cell| {
                let id = event.id();
                let app_menu = cell.get()
                    .expect("Menu not initialized");
                let action = app_menu.map.get(id)
                    .expect(&format!("Action not found for menu_id {id:?}"))
                    .to_owned();
                worker::emit(Message::Window(window::Event::Menu(action)));
            });
        }));
    })
}
