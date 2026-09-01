//! 工具面板 — yinhe `widgets/tools_panel.rs:51` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染工具面板（笔/刷/形状/文字等，对齐 yinhe `widgets/tools_panel.rs:26`）
///
/// - `pencil` → `EDIT f097`，`brush` → `BRUSH e3ae`，`eraser` → `INK_ERASER e6d0`
/// - `shape` → `CATEGORY` 占位（`add e145` 近似矩形），`text` → `TEXT_FIELDS e262`
pub fn view<'a>(window: &'a Window, active_tool: &'a str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let tools = [
        ("pencil", crate::material_icons::codepoints::EDIT),
        ("brush", crate::material_icons::codepoints::BRUSH),
        ("eraser", crate::material_icons::codepoints::INK_ERASER),
        ("shape", crate::material_icons::codepoints::ADD),
        ("text", crate::material_icons::codepoints::TEXT_FIELDS),
    ];

    let buttons: Vec<Element<'a>> = tools
        .iter()
        .map(|(name, cp)| {
            let is_active = *name == active_tool;
            let bg_col = if is_active {
                palette.background.strong.color
            } else {
                iced_core::Color::TRANSPARENT
            };
            let icon_color = if is_active {
                palette.background.strong.text
            } else {
                palette.background.base.text
            };
            button(crate::material_icons::icon(*cp, 16.0, icon_color))
                .on_press(lumino_ui_core::message::null())
                .padding(6)
                .style(move |_t: &Theme, status| {
                    let c = if is_active {
                        palette.background.strong.color
                    } else if status == button::Status::Hovered {
                        palette.background.weak.color
                    } else {
                        iced_core::Color::TRANSPARENT
                    };
                    button::Style {
                        background: Some(iced_core::Background::Color(c)),
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .into()
        })
        .collect();

    container(row(buttons).spacing(4).padding(6))
        .width(Length::Fill)
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
