//! 文本类事件编辑 popup — 对应 yinhe `right_panel/event_browser/edit/text.rs:323`
//!
//! 覆盖 Marker / Lyrics / Chord（conductor 级与 per-track）的
//! `TextEventTick / TextEventText`，yinhe 原以 `Area + TextEdit + DragValue` 实现；
//! iced 桩以 `column + text_input` 占位，保留 `TextEventKind` 分发。

use iced_widget::{column, container, text, text_input};

use lumino_ui_core::{Element, Theme, window::Window};

use super::number_popup_view;
use crate::right_panel::event_browser::state::TextEventKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEditKind {
    Tick,
    Position,
    Text,
}

pub fn view<'a>(
    window: &'a Window,
    kind: TextEditKind,
    event_kind: TextEventKind,
    tick: u32,
    current_text: &str,
) -> Element<'a> {
    let label = match event_kind {
        TextEventKind::Marker => "Marker",
        TextEventKind::ConductorLyrics => "Conductor Lyrics",
        TextEventKind::ConductorChord => "Conductor Chord",
        TextEventKind::Lyrics { track } => &format!("Lyrics (track {track})"),
        TextEventKind::Chord { track } => &format!("Chord (track {track})"),
    };
    // 需 owned 以移入闭包（TextEventKind 含 track 时 label 为临时 String）
    let label = label.to_string();

    match kind {
        TextEditKind::Tick => number_popup_view(
            window,
            format!("Edit {label} tick"),
            tick as f64,
            (0.0, u32::MAX as f64),
        ),
        TextEditKind::Position => {
            super::position_popup_view(window, format!("Edit {label} position"), tick, 1, 0)
        }
        TextEditKind::Text => {
            let palette = window.theme.extended_palette();
            container(
                column![
                    text(format!("Edit {label} text"))
                        .size(11)
                        .style(move |_theme: &Theme| {
                            iced_widget::text::Style {
                                color: Some(palette.background.strong.text),
                            }
                        }),
                    text_input("text…", current_text).padding([4, 6]).size(11),
                    text("Enter to confirm, Esc to cancel")
                        .size(9)
                        .style(move |_theme: &Theme| {
                            iced_widget::text::Style {
                                color: Some(palette.background.weak.text),
                            }
                        }),
                ]
                .spacing(6)
                .padding([8, 8]),
            )
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.base.color)),
                border: iced_core::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn text_popups() {
        let window = Window::new("Tokyo Night Storm");
        let _ = view(
            &window,
            TextEditKind::Text,
            TextEventKind::Marker,
            0,
            "hello",
        );
        let _ = view(
            &window,
            TextEditKind::Tick,
            TextEventKind::Lyrics { track: 1 },
            480,
            "",
        );
    }
}
