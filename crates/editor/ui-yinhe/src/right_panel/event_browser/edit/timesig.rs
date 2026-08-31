//! 拍号编辑 popup — 对应 yinhe `right_panel/event_browser/edit/timesig.rs:211`
//!
//! 覆盖 `TimeSigTick / TimeSigNumerator / TimeSigDenominator` 三类，
//! yinhe 原以 `Area + DragValue + position_popup` 实现；
//! iced 桩以 `number_popup_view / position_popup_view / choice` 占位。

use iced_widget::{column, text};

use lumino_ui_core::{Element, window::Window};

use super::number_popup_view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSigEditKind {
    Tick,
    Position,
    Numerator,
    Denominator,
}

pub fn view<'a>(
    window: &'a Window,
    kind: TimeSigEditKind,
    tick: u32,
    numerator: u8,
    denominator: u8,
) -> Element<'a> {
    match kind {
        TimeSigEditKind::Tick => number_popup_view(
            window,
            "Edit TimeSig tick".to_string(),
            tick as f64,
            (0.0, u32::MAX as f64),
        ),
        TimeSigEditKind::Position => super::position_popup_view(
            window,
            "Edit TimeSig position".to_string().to_string(),
            tick,
            1,
            0,
        ),
        TimeSigEditKind::Numerator => number_popup_view(
            window,
            "Edit TimeSig numerator".to_string(),
            numerator as f64,
            (1.0, 32.0),
        ),
        TimeSigEditKind::Denominator => {
            let denom = 1u32 << denominator as u32;
            column![
                text("Edit TimeSig denominator").size(11),
                text(format!("Current: {denom} (2^{denominator})")).size(11),
                text("Options: 2, 4, 8, 16, 32").size(10),
            ]
            .spacing(4)
            .padding([8, 8])
            .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn timesig_popups() {
        let window = Window::new("Tokyo Night Storm");
        let _ = view(&window, TimeSigEditKind::Tick, 0, 4, 2);
        let _ = view(&window, TimeSigEditKind::Numerator, 0, 4, 2);
        let _ = view(&window, TimeSigEditKind::Denominator, 0, 4, 2);
    }
}
