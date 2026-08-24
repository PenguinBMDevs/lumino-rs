//! 画刷工具下拉（ctrl+点击附属按钮触发）
//!
//! 包含粗细度步进（1-20）与「绘制行为」入口（粗细度>1 时显示，齿轮按钮打开独立对话框）。

use iced_core::{Background, Border, Color, Length};
use iced_widget::{button, column, container, row, space, text};
use lumino_core::BrushConfig;
use lumino_extras::i18n::{Language, main_translations};

use crate::Element;
use crate::message::{BrushSettingsAction, Message};
use crate::resources::icon;
use crate::toolbar::Event;

/// 渲染画刷工具下拉
pub(crate) fn render_brush_dropdown<'a>(
    brush: &'a BrushConfig,
    language: Language,
    panel_background: Color,
    theme: &'a iced_core::Theme,
) -> Element<'a> {
    let _t = main_translations(language);
    let _ = &_t;

    let thickness = brush.thickness;
    let dec = if thickness > 1 {
        thickness - 1
    } else {
        thickness
    };
    let inc = if thickness < BrushConfig::MAX_THICKNESS {
        thickness + 1
    } else {
        thickness
    };

    let mut rows: Vec<Element<'a>> = Vec::new();

    // 粗细度步进
    rows.push(
        container(row![
            text("粗细度").size(14),
            space().width(8),
            button(text("-").size(16))
                .on_press(Event::brush_thickness_changed(dec))
                .padding([2, 10]),
            text(format!("{thickness}"))
                .size(14)
                .width(Length::Fixed(28.0)),
            button(text("+").size(16))
                .on_press(Event::brush_thickness_changed(inc))
                .padding([2, 10]),
            text("/ 20").size(12),
        ])
        .into(),
    );

    // 绘制行为（仅粗细度 > 1 显示）：齿轮按钮打开独立对话框
    if thickness > 1 {
        rows.push(
            container(row![
                text("绘制行为").size(14),
                space().width(8),
                button(icon::view_with_size_and_theme(
                    icon::Gear,
                    18,
                    18,
                    Some(theme)
                ))
                .on_press(Message::BrushSettings(BrushSettingsAction::OpenDialog(
                    brush.clone()
                )))
                .padding(2),
            ])
            .into(),
        );
    }

    let palette = theme.extended_palette();
    container(column(rows).spacing(8).padding(12))
        .style(move |_theme: &iced_core::Theme| container::Style {
            background: Some(Background::Color(panel_background)),
            border: Border {
                width: 1.0,
                color: palette.background.strong.color,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .into()
}
