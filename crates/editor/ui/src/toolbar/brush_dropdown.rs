//! 画刷工具下拉（悬浮条「画刷」条目再次点击触发）
//!
//! 内容：**全局粗细的只读显示** + 「绘制行为」入口（该值 > 1 时显示，齿轮按钮打开
//! 独立对话框，逐层分配音轨）。
//!
//! ⚠️ **这里不再有「粗细度 −/+」步进按钮**：粗细已是**全局**值（曲线 / 形状的描边与
//! 画刷笔画共用），唯一控制入口是悬浮条右端的「粗细」面板滑杆——同一份值摆两个控件
//! （还是两个不同的量程：这里曾硬编码 20）正是"看起来能调、其实两处打架"的那类坑。
//! 本下拉保留一个只读的当前值，是为了让用户在画刷语境里也能看到笔头多粗、并据此
//! 决定要不要点开「绘制行为」。
//!
//! 布局规则：顶部为粗细信息行，交互按钮（绘制行为）统一沉到底部一行；
//! 与「绘制工具选择面板」(`tool_panel.rs`) 同属曲线工具下拉，圆角对齐为 8。

use iced_core::{Alignment, Background, Border, Color};
use iced_widget::{button, column, container, row, space, text};
use lumino_ui_core::color::contrast_text_color;

use crate::Element;
use crate::message::{BrushSettingsAction, Message};
use crate::resources::icon;
use lumino_core::BrushConfig;
use lumino_extras::i18n::{Language, main_translations};

/// 渲染画刷工具下拉
pub(crate) fn render_brush_dropdown<'a>(
    brush: &'a BrushConfig,
    language: Language,
    panel_background: Color,
    theme: &'a iced_core::Theme,
) -> Element<'a> {
    let t = main_translations(language);

    // 全局粗细（只读展示；量程与「粗细」面板滑杆同源，见 `thickness_dropdown.rs`）
    let thickness = brush
        .thickness
        .clamp(BrushConfig::MIN_THICKNESS, BrushConfig::MAX_THICKNESS);

    // 顶部信息行：标题 + 当前值 / 上限
    let info_row = row![
        text(t.tool_thickness).size(14),
        space().width(8),
        text(format!(
            "{thickness} {} / {}",
            t.draw_thickness_unit,
            BrushConfig::MAX_THICKNESS
        ))
        .size(14),
    ];

    // 底部按钮行：绘制行为入口（逐层音轨分配）
    let mut buttons: Vec<Element<'a>> = Vec::new();
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
