//! Program Change 编辑 popup — 对应 yinhe `right_panel/event_browser/edit/pc.rs:164`
//!
//! 覆盖 `PcTick / PcProgram` 两类，yinhe 原以 `Area + DragValue` 实现；
//! iced 桩以 `number_popup_view / position_popup_view` 占位。

use lumino_ui_core::{Element, window::Window};

use super::{number_popup_view, position_popup_view};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcEditKind {
    Tick,
    Position,
    Program,
}

pub fn view<'a>(window: &'a Window, kind: PcEditKind, tick: u32, program: u8) -> Element<'a> {
    match kind {
        PcEditKind::Tick => number_popup_view(
            window,
            "Edit PC tick".to_string(),
            tick as f64,
            (0.0, u32::MAX as f64),
        ),
        PcEditKind::Position => {
            position_popup_view(window, "Edit PC position".to_string(), tick, 1, 0)
        }
        PcEditKind::Program => number_popup_view(
            window,
            "Edit PC program".to_string(),
            program as f64,
            (0.0, 127.0),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn pc_popups() {
        let window = Window::new("Tokyo Night Storm");
        let _ = view(&window, PcEditKind::Tick, 0, 1);
        let _ = view(&window, PcEditKind::Program, 0, 42);
    }
}
