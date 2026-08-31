//! 量化弹窗 — yinhe `widgets/quantize_popup.rs:98` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text, text_input};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 量化预设（对齐 `yinhe_editor_core::quantize::QuantizePreset` 的展示子集）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantizePreset {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    Fraction(u32, u32),
    Absolute(u32),
}

impl QuantizePreset {
    pub const ALL: &[Self] = &[
        Self::Whole,
        Self::Half,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::ThirtySecond,
    ];

    pub fn display(&self, _ppq: u32) -> String {
        match self {
            Self::Whole => "1/1".to_string(),
            Self::Half => "1/2".to_string(),
            Self::Quarter => "1/4".to_string(),
            Self::Eighth => "1/8".to_string(),
            Self::Sixteenth => "1/16".to_string(),
            Self::ThirtySecond => "1/32".to_string(),
            Self::Fraction(n, d) => format!("{n}/{d}"),
            Self::Absolute(t) => format!("{t} tick"),
        }
    }
}

fn menu_item<'a>(window: &'a Window, label: String, selected: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = if selected {
        palette.background.strong.color
    } else {
        iced_core::Color::TRANSPARENT
    };
    let txt = if selected {
        palette.background.strong.text
    } else {
        palette.background.base.text
    };
    button(
        text(label)
            .size(12)
            .style(move |_t: &Theme| iced_widget::text::Style { color: Some(txt) }),
    )
    .on_press(lumino_ui_core::message::null())
    .width(Length::Fill)
    .padding([6, 10])
    .style(move |_t: &Theme, _| button::Style {
        background: Some(iced_core::Background::Color(bg)),
        border: iced_core::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// 渲染量化弹窗
pub fn view<'a>(window: &'a Window, ppq: u32, current: QuantizePreset) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let mut items: Vec<Element<'a>> = Vec::new();
    for preset in QuantizePreset::ALL {
        items.push(menu_item(window, preset.display(ppq), *preset == current));
    }
    items.push(
        container(iced_widget::Space::new().height(1))
            .width(Length::Fill)
            .style(move |_t: &Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.weak.color)),
                ..Default::default()
            })
            .into(),
    );
    let is_frac = matches!(current, QuantizePreset::Fraction(_, _));
    items.push(menu_item(window, "custom_fraction".to_string(), is_frac));
    if let QuantizePreset::Fraction(n, d) = current {
        items.push(
            row![
                text("numerator").size(11),
                text_input("1", &n.to_string())
                    .on_input(|_| lumino_ui_core::message::null())
                    .padding(4)
                    .width(Length::Fixed(60.0)),
                text("denominator").size(11),
                text_input("1", &d.to_string())
                    .on_input(|_| lumino_ui_core::message::null())
                    .padding(4)
                    .width(Length::Fixed(60.0)),
            ]
            .spacing(6)
            .into(),
        );
    }
    items.push(
        container(iced_widget::Space::new().height(1))
            .width(Length::Fill)
            .style(move |_t: &Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.weak.color)),
                ..Default::default()
            })
            .into(),
    );
    let is_abs = matches!(current, QuantizePreset::Absolute(_));
    items.push(menu_item(window, "custom_tick".to_string(), is_abs));
    if let QuantizePreset::Absolute(v) = current {
        items.push(
            text_input("1", &v.to_string())
                .on_input(|_| lumino_ui_core::message::null())
                .padding(4)
                .width(Length::Fixed(80.0))
                .into(),
        );
    }

    container(column(items).spacing(2).padding(6))
        .width(Length::Fixed(200.0))
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: palette.background.weak.color,
            },
            ..Default::default()
        })
        .into()
}
