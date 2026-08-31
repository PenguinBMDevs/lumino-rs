//! macOS 平台适配 — yinhe `platform/macos.rs:1064` 的 iced 迁移桩
//!
//! - 复用 `muda` 的 `Menu` / `MenuItem` / `CheckMenuItem` 构建原生 `NSMenu`
//!   （与 yinhe `platform/macos.rs` 同款 `init_native_menu` / `MenuEvent::set_event_handler`）
//! - 菜单项文本 `t!(key)` 占位（实际由 `lumino` 的 i18n 提供，此处以 key 本身示意）
//! - 字体/配色走 `Theme`（不引入 `egui`），图标走 SVG `define_icons!`
//! - `setDocumentEdited:` / `requestUserAttention:` / `App Nap` 等 `objc2` 调用
//!   在 iced 侧以 `winit` 的 `Window` 句柄可接入时再恢复；桩层先以 `tracing` 占位

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, mpsc};

use muda::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};

use super::MenuAction;

// ── 全局状态（与 yinhe 同款 OnceLock + Mutex 保活） ──

static MENU_SENDER: Mutex<Option<mpsc::Sender<MenuAction>>> = Mutex::new(None);
static MENU_MAP: OnceLock<Mutex<HashMap<MenuId, MenuAction>>> = OnceLock::new();

thread_local! {
    static NATIVE_MENU: OnceLock<NativeMenu> = const { OnceLock::new() };
}

trait MenuText: IsMenuItem {
    fn set_text(&self, text: &str);
    fn update_accelerator(&self, accelerator: Option<Accelerator>);
}

impl MenuText for MenuItem {
    fn set_text(&self, text: &str) {
        MenuItem::set_text(self, text);
    }
    fn update_accelerator(&self, accelerator: Option<Accelerator>) {
        let _ = MenuItem::set_accelerator(self, accelerator);
    }
}

impl MenuText for Submenu {
    fn set_text(&self, text: &str) {
        Submenu::set_text(self, text);
    }
    fn update_accelerator(&self, _: Option<Accelerator>) {}
}

impl MenuText for PredefinedMenuItem {
    fn set_text(&self, _: &str) {}
    fn update_accelerator(&self, _: Option<Accelerator>) {}
}

struct NativeMenu {
    _menu: Menu,
    _items: Vec<(&'static str, Box<dyn MenuText>)>,
    recent_submenu: Submenu,
    recent_items: std::cell::RefCell<Vec<MenuItem>>,
}

fn code_for_key(s: &str) -> Option<Code> {
    Some(match s {
        "A" => Code::KeyA,
        "S" => Code::KeyS,
        "Z" => Code::KeyZ,
        "Space" => Code::Space,
        "Comma" => Code::Comma,
        _ => return None,
    })
}

fn init_native_menu() -> muda::Result<()> {
    let mut map = HashMap::new();
    let mut items: Vec<(&'static str, Box<dyn MenuText>)> = Vec::new();
    let cmd = Modifiers::SUPER;

    // App 菜单
    let about = Box::new(MenuItem::new("About", true, None));
    map.insert(about.id().clone(), MenuAction::About);
    let settings = Box::new(MenuItem::new(
        "Settings",
        true,
        Some(Accelerator::new(Some(cmd), Code::Comma)),
    ));
    map.insert(settings.id().clone(), MenuAction::Settings);
    let hide = Box::new(MenuItem::new(
        "Hide",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyH)),
    ));
    map.insert(hide.id().clone(), MenuAction::Hide);
    let hide_others = Box::new(MenuItem::new(
        "Hide Others",
        true,
        Some(Accelerator::new(Some(cmd | Modifiers::SHIFT), Code::KeyH)),
    ));
    map.insert(hide_others.id().clone(), MenuAction::HideOthers);
    let show_all = Box::new(MenuItem::new("Show All", true, None));
    map.insert(show_all.id().clone(), MenuAction::ShowAll);
    let quit = Box::new(MenuItem::new(
        "Quit",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyQ)),
    ));
    map.insert(quit.id().clone(), MenuAction::Exit);
    let sep = PredefinedMenuItem::separator();
    let app_items: Vec<&dyn IsMenuItem> = vec![
        about.as_ref(),
        &sep,
        settings.as_ref(),
        &sep,
        hide.as_ref(),
        hide_others.as_ref(),
        show_all.as_ref(),
        &sep,
        quit.as_ref(),
    ];
    let app_menu = Submenu::with_items("Yinhe", true, &app_items)?;

    // 文件菜单（简化）
    let new_item = Box::new(MenuItem::new(
        "New Project",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyN)),
    ));
    map.insert(new_item.id().clone(), MenuAction::NewProject);
    let open_item = Box::new(MenuItem::new(
        "Open",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyO)),
    ));
    map.insert(open_item.id().clone(), MenuAction::Open);
    let save_item = Box::new(MenuItem::new(
        "Save",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyS)),
    ));
    map.insert(save_item.id().clone(), MenuAction::Save);
    let file_items: Vec<&dyn IsMenuItem> =
        vec![new_item.as_ref(), open_item.as_ref(), save_item.as_ref()];
    let file_menu = Submenu::with_items("File", true, &file_items)?;
    let recent_submenu = Submenu::new("Recent Files", false);
    let _ = file_menu.insert(&recent_submenu, 2);

    // 编辑菜单（简化）
    let undo = Box::new(MenuItem::new(
        "Undo",
        true,
        Some(Accelerator::new(Some(cmd), Code::KeyZ)),
    ));
    map.insert(undo.id().clone(), MenuAction::Undo);
    let redo = Box::new(MenuItem::new(
        "Redo",
        true,
        Some(Accelerator::new(Some(cmd | Modifiers::SHIFT), Code::KeyZ)),
    ));
    map.insert(redo.id().clone(), MenuAction::Redo);
    let edit_items: Vec<&dyn IsMenuItem> = vec![undo.as_ref(), redo.as_ref()];
    let edit_menu = Submenu::with_items("Edit", true, &edit_items)?;

    // 播放菜单
    let play = Box::new(MenuItem::new(
        "Play/Pause",
        true,
        code_for_key("Space").map(|c| Accelerator::new(None, c)),
    ));
    map.insert(play.id().clone(), MenuAction::TogglePlay);
    let play_items: Vec<&dyn IsMenuItem> = vec![play.as_ref()];
    let play_menu = Submenu::with_items("Playback", true, &play_items)?;

    let menu_items: Vec<&dyn IsMenuItem> = vec![&app_menu, &file_menu, &edit_menu, &play_menu];
    let menu = Menu::with_items(&menu_items)?;
    menu.init_for_nsapp();

    items.push(("menu.about", about));
    items.push(("menu.settings", settings));
    items.push(("menu.hide", hide));
    items.push(("menu.hide_others", hide_others));
    items.push(("menu.show_all", show_all));
    items.push(("menu.quit", quit));
    items.push(("menu.file", Box::new(file_menu)));
    items.push(("menu.edit", Box::new(edit_menu)));
    items.push(("menu.playback", Box::new(play_menu)));
    items.push(("menu.app", Box::new(app_menu)));

    let _ = MENU_MAP.set(Mutex::new(map));
    NATIVE_MENU.with(|cell| {
        let _ = cell.set(NativeMenu {
            _menu: menu,
            _items: items,
            recent_submenu,
            recent_items: std::cell::RefCell::new(Vec::new()),
        });
    });

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(map_lock) = MENU_MAP.get()
            && let Ok(map) = map_lock.lock()
            && let Some(action) = map.get(event.id())
        {
            if let Ok(guard) = MENU_SENDER.lock()
                && let Some(tx) = guard.as_ref()
            {
                let _ = tx.send(action.clone());
            }
        }
    }));

    Ok(())
}

pub(crate) struct MenuBarInner {
    rx: mpsc::Receiver<MenuAction>,
    open_files_rx: mpsc::Receiver<String>,
}

impl MenuBarInner {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        *MENU_SENDER.lock().expect("menu sender poisoned") = Some(tx);
        let (_open_tx, open_rx) = mpsc::channel();
        if let Err(e) = init_native_menu() {
            tracing::error!("init macOS menu failed: {e:?}");
        }
        Self {
            rx,
            open_files_rx: open_rx,
        }
    }

    pub fn poll(&mut self) -> Vec<MenuAction> {
        std::iter::from_fn(|| self.rx.try_recv().ok()).collect()
    }

    pub fn poll_open_files(&mut self) -> Vec<String> {
        std::iter::from_fn(|| self.open_files_rx.try_recv().ok()).collect()
    }
}

pub(crate) fn set_document_edited(_edited: bool) {
    tracing::trace!("set_document_edited (stub)");
}

pub(crate) fn request_user_attention() {
    tracing::trace!("request_user_attention (stub)");
}

pub(crate) fn set_app_nap_enabled(_enabled: bool) {
    tracing::trace!("set_app_nap_enabled (stub)");
}

pub(crate) fn disable_background_window_drag() {
    tracing::trace!("disable_background_window_drag (stub)");
}
