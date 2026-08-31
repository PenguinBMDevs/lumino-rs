//! 自动化锚点信息面板 — 对应 yinhe `right_panel/info_panel/anchor.rs:435`
//!
//! 显示选中锚点的 Tick / Value / Shape / X1 / Y1 / X2 / Y2 编辑器，
//! 并通过 `LaneUndoGuard` 语义（DragValue focus/before/after）在 iced 侧
//! 以 `Message` 单向流重构。yinhe 原六处 `LaneUndoGuard` 样板在 iced 桩中
//! 收敛为统一的 `anchor_field_row`。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::{Element, Theme, window::Window};

/// 曲线形貌（对齐 yinhe `SegmentShape`）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnchorShape {
    Step,
    Linear,
    Curve { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl Default for AnchorShape {
    fn default() -> Self {
        Self::Step
    }
}

impl AnchorShape {
    #[must_use]
    pub fn is_linear(self) -> bool {
        matches!(self, Self::Linear)
            || matches!(
                self,
                Self::Curve { x1, y1, x2, y2 } if x1 == 0.0 && y1 == 0.0 && x2 == 0.0 && y2 == 0.0
            )
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Step => "Step",
            Self::Linear => "Linear",
            Self::Curve { .. } if self.is_linear() => "Linear",
            Self::Curve { .. } => "Curve",
        }
    }

    #[must_use]
    pub fn desc(self) -> &'static str {
        match self {
            Self::Step => "Step: hold until next anchor",
            Self::Linear => "Linear: straight interpolation",
            Self::Curve { .. } => "Curve: Bezier with handles",
        }
    }
}

/// 锚点信息（对齐 yinhe `InfoContent::Anchor { track_idx, lane_idx, event_idx, target }`）
///
/// `target_name` 为显示名（如 `"CC 7"` / `"PitchBend"` / `"Tempo"`），
/// `max_value` 来自 `AutomationTarget::max_value()`。
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorInfo {
    pub track_idx: u16,
    pub lane_idx: usize,
    pub tick: u32,
    pub value: f32,
    pub shape: AnchorShape,
    pub target_name: String,
    pub max_value: f32,
}

impl Default for AnchorInfo {
    fn default() -> Self {
        Self {
            track_idx: 0,
            lane_idx: 0,
            tick: 0,
            value: 0.0,
            shape: AnchorShape::Step,
            target_name: "CC 7".to_string(),
            max_value: 127.0,
        }
    }
}

fn small_label<'a>(window: &'a Window, s: impl Into<String>) -> Element<'a> {
    let palette = window.theme.extended_palette();
    text(s.into())
        .size(11)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(palette.background.weak.text),
        })
        .into()
}

fn field_row<'a>(window: &'a Window, label: &'a str, value: String, range: String) -> Element<'a> {
    let palette = window.theme.extended_palette();
    row![
        text(label).size(11).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }
        }),
        container(text(value).size(11))
            .padding([2, 6])
            .width(Length::Fixed(80.0))
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.weak.color)),
                border: iced_core::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        text(range).size(9).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }
        }),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// 锚点面板 `view()` — Tick / Value / Shape + （Curve 时）X1/Y1/X2/Y2
pub fn view<'a>(window: &'a Window, info: &'a AnchorInfo) -> Element<'a> {
    let palette = window.theme.extended_palette();

    let title = text("Anchor")
        .size(13)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(palette.background.strong.text),
        });

    let target_row = row![
        small_label(window, "Target:"),
        text(info.target_name.clone())
            .size(12)
            .style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.strong.text),
                }
            }),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let tick_row = field_row(window, "Tick:", info.tick.to_string(), "0..MAX".to_string());
    let value_row = field_row(
        window,
        "Value:",
        format!("{:.2}", info.value),
        format!("0..{:.0}", info.max_value),
    );

    let shape_row = row![
        small_label(window, "Shape:"),
        text(info.shape.label()).size(11),
        button(text("Discrete").size(11)).padding([2, 6]),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let curve_rows: Element<'a> = match info.shape {
        AnchorShape::Curve { x1, y1, x2, y2 } => column![
            field_row(window, "X1:", format!("{x1:.2}"), "0..0.25".to_string()),
            field_row(window, "Y1:", format!("{y1:.2}"), "-0.5..0.5".to_string()),
            field_row(window, "X2:", format!("{x2:.2}"), "-0.25..0".to_string()),
            field_row(window, "Y2:", format!("{y2:.2}"), "-0.5..0.5".to_string()),
        ]
        .spacing(4)
        .into(),
        _ => column![].into(),
    };

    let desc = text(info.shape.desc())
        .size(10)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(palette.background.weak.text),
        });

    column![
        title,
        target_row,
        tick_row,
        value_row,
        shape_row,
        curve_rows,
        desc,
        button(text("Clear selection").size(11)).padding([4, 8]),
    ]
    .spacing(6)
    .padding([8, 8])
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn anchor_view_step() {
        let window = Window::new("Tokyo Night Storm");
        let info = AnchorInfo {
            tick: 480,
            value: 64.0,
            shape: AnchorShape::Step,
            ..Default::default()
        };
        let _el = view(&window, &info);
    }

    #[test]
    fn anchor_view_curve() {
        let window = Window::new("Tokyo Night Storm");
        let info = AnchorInfo {
            tick: 960,
            value: 100.0,
            shape: AnchorShape::Curve {
                x1: 0.1,
                y1: 0.2,
                x2: -0.1,
                y2: -0.2,
            },
            ..Default::default()
        };
        let _el = view(&window, &info);
    }
}
