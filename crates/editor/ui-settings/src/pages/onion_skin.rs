//! 设置页面 - 洋葱皮设置

use iced_core::Alignment;
use iced_widget::{column, row, text, text_input};
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::create_content_text_style;
use super::super::components::{with_setting_tooltip_inline, with_setting_tooltip_inline_action};
use crate::SettingsPanel;

/// 渲染洋葱皮设置页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    column![
        text("洋葱皮设置")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 启用高精度贴图开关（说明挂在标签文字上，提示不覆盖对钩框；点击文字等价于切换对钩）
        row![
            iced_widget::Checkbox::new(settings.hires.onion_enabled).on_toggle(|enabled| {
                Message::Settings(crate::Event::HiresOnionEnabledChanged(enabled))
            }),
            with_setting_tooltip_inline_action(
                text("启用高精度洋葱皮贴图")
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                "启用后将在钢琴卷帘上叠加高精度预渲染贴图，编辑后自动重生成。",
                Message::Settings(crate::Event::HiresOnionEnabledChanged(
                    !settings.hires.onion_enabled,
                )),
            ),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(24),
        // 组内小节数
        row![
            with_setting_tooltip_inline(
                text("每组小节数 (1-16)")
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                "组内小节数越大单张贴图越长，内存占用越高。",
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("4", &settings.hires.measures_per_group.to_string())
                .on_input(|v| Message::Settings(crate::Event::HiresMeasuresPerGroupChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 贴图宽度
        row![
            with_setting_tooltip_inline(
                text("贴图宽度像素 (480-7680)")
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                "贴图宽度越大越清晰但内存占用越高，建议不超过 3840。",
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("1920", &settings.hires.tile_width_px.to_string())
                .on_input(|v| Message::Settings(crate::Event::HiresTileWidthChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 冷静期秒数
        row![
            with_setting_tooltip_inline(
                text("编辑后重生成冷静期秒数 (3-60)")
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                "停止编辑后等待指定秒数再重生成贴图，避免频繁重绘。",
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("10", &settings.hires.cooldown_secs.to_string())
                .on_input(|v| Message::Settings(crate::Event::HiresCooldownChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // GPU 显存上限
        row![
            with_setting_tooltip_inline(
                text("GPU 显存上限 MB (128-4096)")
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                "GPU 显存上限越大可同时显示越多贴图。",
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("512", &settings.hires.gpu_mem_limit_mb.to_string())
                .on_input(|v| Message::Settings(crate::Event::HiresGpuMemLimitChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
