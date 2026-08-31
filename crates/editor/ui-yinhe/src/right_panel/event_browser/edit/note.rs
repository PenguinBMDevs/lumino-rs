//! 音符编辑 popup — 对应 yinhe `right_panel/event_browser/edit/note.rs:301`
//!
//! 覆盖 `NoteStartTick / NoteEndTick / NoteGate / NoteKey / NoteVelocity` 五类，
//! yinhe 原以 `egui::Area + DragValue` 实现，`gate` 实际改 `end_tick = start + gate`；
//! iced 桩以 `number_popup_view / position_popup_view` 占位，保留 `NoteRef` 寻址。

use lumino_ui_core::{Element, window::Window};

use super::{number_popup_view, position_popup_view};
use crate::right_panel::event_browser::state::NoteRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteEditKind {
    StartTick,
    StartPosition,
    EndTick,
    EndPosition,
    Gate,
    Key,
    Velocity,
}

pub fn view<'a>(window: &'a Window, kind: NoteEditKind, note: NoteRef) -> Element<'a> {
    match kind {
        NoteEditKind::StartTick => number_popup_view(
            window,
            "Edit note start tick".to_string(),
            note.start_tick as f64,
            (0.0, u32::MAX as f64),
        ),
        NoteEditKind::StartPosition => position_popup_view(
            window,
            "Edit note start position".to_string(),
            note.start_tick,
            1,
            0,
        ),
        NoteEditKind::EndTick => number_popup_view(
            window,
            "Edit note end tick".to_string(),
            note.end_tick as f64,
            (note.start_tick as f64 + 1.0, u32::MAX as f64),
        ),
        NoteEditKind::EndPosition => position_popup_view(
            window,
            "Edit note end position".to_string(),
            note.end_tick,
            1,
            0,
        ),
        NoteEditKind::Gate => {
            let gate = note.end_tick.saturating_sub(note.start_tick);
            number_popup_view(
                window,
                "Edit note gate".to_string(),
                gate as f64,
                (1.0, u32::MAX as f64),
            )
        }
        NoteEditKind::Key => number_popup_view(
            window,
            "Edit note key".to_string(),
            note.key as f64,
            (0.0, 127.0),
        ),
        NoteEditKind::Velocity => number_popup_view(
            window,
            "Edit note velocity".to_string(),
            note.velocity as f64,
            (0.0, 127.0),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    fn sample_note() -> NoteRef {
        NoteRef {
            id: 1,
            start_tick: 0,
            end_tick: 480,
            key: 60,
            velocity: 100,
            track: 0,
        }
    }

    #[test]
    fn note_popups() {
        let window = Window::new("Tokyo Night Storm");
        let n = sample_note();
        let _ = view(&window, NoteEditKind::StartTick, n);
        let _ = view(&window, NoteEditKind::Gate, n);
        let _ = view(&window, NoteEditKind::Key, n);
        let _ = view(&window, NoteEditKind::Velocity, n);
    }
}
