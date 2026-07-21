//! 设置页面 - 编辑设置
//!
//! 包含操作历史（撤销/重做日志上限、合并窗口）和编辑拦截（Toast 提示）。

use iced_core::Alignment;
use iced_widget::{column, row, text, text_input};
use lumino_core::i18n::settings_translations;
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::SettingsPanel;

/// 渲染编辑设置页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.language);

    column![
        text(t.editing_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // ── 操作历史 section ──
        text(t.editing_history_section)
            .size(TEXT_SIZE_SECTION)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 操作日志总条数上限
        row![
            text(t.editing_history_total_limit)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("100", &settings.history_total_limit.to_string())
                .on_input(|v| Message::Settings(crate::Event::HistoryTotalLimitChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_history_total_limit_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 单条日志条目上限
        row![
            text(t.editing_history_entry_limit)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("1000", &settings.history_entry_limit.to_string())
                .on_input(|v| Message::Settings(crate::Event::HistoryEntryLimitChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_history_entry_limit_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 合并窗口
        row![
            text(t.editing_merge_window)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("300", &settings.merge_window_ms.to_string())
                .on_input(|v| Message::Settings(crate::Event::MergeWindowMsChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_merge_window_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // ── 编辑拦截 section ──
        text(t.editing_intercept_section)
            .size(TEXT_SIZE_SECTION)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 拦截时显示 Toast 提示
        row![
            iced_widget::Checkbox::new(settings.intercept_notification_enabled)
                .label(t.editing_intercept_notification)
                .on_toggle(|enabled| {
                    Message::Settings(crate::Event::InterceptNotificationChanged(enabled))
                }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_intercept_notification_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // ── 自动化曲线连线粗细 section ──
        row![
            text(t.editing_automation_line_thickness)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(200.0),
            iced_widget::slider(1.0..=10.0, settings.automation_line_thickness, |v| {
                Message::Settings(crate::Event::AutomationLineThicknessChanged(v))
            })
            .step(0.5_f32)
            .width(200.0),
            text(format!("{:.1} px", settings.automation_line_thickness))
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(50.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_automation_line_thickness_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
