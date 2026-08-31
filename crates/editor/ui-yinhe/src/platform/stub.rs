//! 非 macOS 桩

use super::MenuAction;
use std::sync::mpsc;

pub(crate) struct MenuBarInner {
    rx: mpsc::Receiver<MenuAction>,
}

impl MenuBarInner {
    pub fn new() -> Self {
        let (_tx, rx) = mpsc::channel();
        Self { rx }
    }

    pub fn poll(&mut self) -> Vec<MenuAction> {
        Vec::new()
    }

    pub fn poll_open_files(&mut self) -> Vec<String> {
        Vec::new()
    }
}

pub(crate) fn set_document_edited(_edited: bool) {}
pub(crate) fn request_user_attention() {}
pub(crate) fn set_app_nap_enabled(_enabled: bool) {}
pub(crate) fn disable_background_window_drag() {}
