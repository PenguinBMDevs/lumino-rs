//! 调号编辑 popup — 对应 yinhe `right_panel/event_browser/edit/keysig.rs:210`
//!
//! 覆盖 `KeySigTick / KeySigRoot / KeySigScale` 三类，
//! yinhe 原以 `egui::Area + ComboBox + DragValue` 实现；
//! iced 桩以 `number_popup_view / choice_popup` 占位。

use iced_widget::{column, text};

use lumino_ui_core::{Element, window::Window};

use super::number_popup_view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySigEditKind {
    Tick,
    Root,
    Scale,
}

const ROOT_NAMES: [&str; 12] = [
    "C", "C#/Db", "D", "D#/Eb", "E", "F", "F#/Gb", "G", "G#/Ab", "A", "A#/Bb", "B",
];

pub fn view<'a>(
    window: &'a Window,
    kind: KeySigEditKind,
    tick: u32,
    root: u8,
    scale_name: &str,
) -> Element<'a> {
    match kind {
        KeySigEditKind::Tick => number_popup_view(
            window,
            "Edit KeySig tick".to_string(),
            tick as f64,
            (0.0, u32::MAX as f64),
        ),
        KeySigEditKind::Root => {
            let display = format!("{} ({})", ROOT_NAMES[root as usize % 12], root);
            column![
                text("Edit KeySig root").size(11),
                text(display).size(11),
                text("Options: C, C#/Db, D, … B").size(10),
            ]
            .spacing(4)
            .padding([8, 8])
            .into()
        }
        KeySigEditKind::Scale => column![
            text("Edit KeySig scale").size(11),
            text(scale_name.to_string()).size(11),
            text("Options: Major / Minor / Dorian / …").size(10),
        ]
        .spacing(4)
        .padding([8, 8])
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn keysig_popups() {
        let window = Window::new("Tokyo Night Storm");
        let _ = view(&window, KeySigEditKind::Tick, 0, 0, "Major");
        let _ = view(&window, KeySigEditKind::Root, 0, 2, "Major");
        let _ = view(&window, KeySigEditKind::Scale, 0, 0, "Dorian");
    }
}
