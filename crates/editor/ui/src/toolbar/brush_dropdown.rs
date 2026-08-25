//! 画刷工具下拉（ctrl+点击附属按钮触发）
//!
//! 包含粗细度步进（1-20）与「绘制行为」入口（粗细度>1 时显示，齿轮按钮打开独立对话框）。
//!
//! 布局规则：顶部为粗细度信息行，所有交互按钮（− / + / 绘制行为）统一沉到底部一行；
//! 与「绘制工具选择面板」(`tool_panel.rs`) 同属曲线工具下拉，圆角对齐为 8。

use iced_core::{Alignment, Background, Border, Color};
use iced_widget::{button, column, container, row, space, text};
use lumino_ui_core::color::contrast_text_color;

use crate::Element;
use crate::message::{BrushSettingsAction, Message};
use crate::resources::icon;
use crate::toolbar::Event;
use lumino_core::BrushConfig;
use lumino_extras::i18n::{Language, main_translations};

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

    // 顶部信息行：粗细度标题 + 当前值 / 上限
    let info_row = row![
        text("粗细度").size(14),
        space().width(8),
        text(format!("{thickness} / {}", BrushConfig::MAX_THICKNESS)).size(14),
    ];

    // 底部按钮行：所有交互按钮统一沉到末尾（步进 + 绘制行为入口）
    let mut buttons: Vec<Element<'a>> = Vec::new();
    buttons.push(
        button(text("-").size(16))
            .on_press(Event::brush_thickness_changed(dec))
            .padding([2, 10])
            .into(),
    );
    buttons.push(
        button(text("+").size(16))
            .on_press(Event::brush_thickness_changed(inc))
            .padding([2, 10])
            .into(),
    );
    if thickness > 1 {
        buttons.push(
            button(
                row![
                    icon::view_with_size_and_theme(icon::Gear, 18, 18, Some(theme)),
                    space().width(4),
                    text("绘制行为").size(14),
                ]
                .align_y(Alignment::Center),
            )
            .on_press(Message::BrushSettings(BrushSettingsAction::OpenDialog(
                brush.clone(),
            )))
            .padding(4)
            .into(),
        );
    }
    let button_row = row(buttons).spacing(8).align_y(Alignment::Center);

    let palette = theme.extended_palette();
    // 面板背景由工具栏底色派生，在亮色模式下可能仍为偏暗色；
    // 文字颜色必须按实际背景亮度计算，否则亮色模式下黑字落在暗面板上不可见。
    let panel_text_color = contrast_text_color(panel_background);
    container(
        column![info_row, button_row]
            .spacing(8)
            .align_x(Alignment::Start)
            .padding(12),
    )
    .style(move |_theme: &iced_core::Theme| container::Style {
        background: Some(Background::Color(panel_background)),
        text_color: Some(panel_text_color),
        border: Border {
            width: 1.0,
            color: palette.background.strong.color,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
    .into()
}
