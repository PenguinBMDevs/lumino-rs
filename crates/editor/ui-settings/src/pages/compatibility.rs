//! 设置页面 - 兼容性（GPU 兼容性检查与启动警告）。
//!
//! 页面只负责展示与发起事件；检查执行在 Runner（后台线程），
//! 结果经事件总线回传并注入本页状态（`GpuCheckUiState`）。

use iced_widget::{button, checkbox, column, row, text};
use lumino_extras::i18n::settings_translations;
use lumino_ui_core::{Element, Message, state::GpuCheckUiState};

use crate::SettingsPanel;

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};

/// 渲染兼容性页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.display.language);
    let compat = &settings.compat;

    // 手动检查按钮：检查中禁用并显示"检查中…"
    let is_running = matches!(compat.check_state, GpuCheckUiState::Running);
    let check_label = if is_running {
        t.compat_checking
    } else {
        t.compat_check_button
    };
    let check_button = {
        let btn = button(text(check_label).size(TEXT_SIZE_CONTENT));
        if is_running {
            btn
        } else {
            btn.on_press(Message::Settings(crate::Event::RunGpuCompatibilityCheck))
        }
    };

    // 复制诊断按钮：仅在已有结果时可用
    let has_result = matches!(compat.check_state, GpuCheckUiState::Done(_));
    let copy_label = if compat.copied {
        t.compat_copied
    } else {
        t.compat_copy_diagnostics
    };
    let copy_button = {
        let btn = button(text(copy_label).size(TEXT_SIZE_CONTENT));
        if has_result {
            btn.on_press(Message::Settings(crate::Event::CopyGpuDiagnostics))
        } else {
            btn
        }
    };

    let result_block: Element<'a> = match &compat.check_state {
        GpuCheckUiState::Idle => text(t.compat_never_checked)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style())
            .into(),
        GpuCheckUiState::Running => text(t.compat_checking)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style())
            .into(),
        GpuCheckUiState::Done(result) => {
            let headline = if result.passed {
                t.compat_passed
            } else {
                t.compat_failed
            };
            column![
                text(headline)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                text(result.detail.clone())
                    .size(12.0)
                    .style(create_placeholder_text_style()),
            ]
            .spacing(4)
            .into()
        }
    };

    column![
        text(t.compatibility_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        row![check_button, copy_button].spacing(12),
        iced_widget::space().height(8),
        result_block,
        iced_widget::space().height(20),
        checkbox(compat.check_on_startup)
            .label(t.compat_check_on_startup)
            .on_toggle(
                |enabled| Message::Settings(crate::Event::GpuCheckOnStartupChanged(enabled,))
            ),
        text(t.compat_check_on_startup_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(8),
        checkbox(!compat.warning_suppressed)
            .label(t.compat_show_warning)
            .on_toggle(
                |visible| Message::Settings(crate::Event::GpuWarningSuppressedChanged(!visible),)
            ),
        text(t.compat_show_warning_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
