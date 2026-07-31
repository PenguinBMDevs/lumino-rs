//! 音频导出对话框
//!
//! 重构说明：
//! - 所有 section 函数返回 `crate::Element`，通过 `.into()` 让 Renderer 类型推断为 `iced_wgpu`。
//! - 样式闭包每次都从 `widgets::dialog_*_style(palette)` 创建新闭包，
//!   避免借用局部变量导致 E0515。
//! - 此文件为模块入口，子模块位于 audio_export_dialog/ 目录下。

mod title;
mod project_info;
mod audio_settings;
mod event_filter;
mod output_path;
mod buttons;

use iced_widget::{column, container, scrollable, space};

use crate::state::root_state::AudioExportDialogState;

use self::{
    audio_settings::audio_settings_section, buttons::buttons_section,
    event_filter::event_filter_section, output_path::output_path_section,
    project_info::project_info_section, title::title_section,
};

/// 渲染音频导出对话框
pub fn view_audio_export_dialog<'a>(
    state: &'a AudioExportDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let main_content = column![
        title_section(palette),
        space().height(16),
        project_info_section(state, palette),
        space().height(16),
        audio_settings_section(state, palette),
        space().height(16),
        event_filter_section(state, palette),
        space().height(16),
        output_path_section(state, palette),
        space().height(24),
        buttons_section(state, palette),
    ];

    let scrollable_content = scrollable(main_content)
        .width(iced_core::Length::Fill)
        .height(iced_core::Length::Fill);

    let dialog_content = container(scrollable_content)
        .width(iced_core::Length::Fill)
        .height(iced_core::Length::Fill)
        .padding(24)
        .style(move |_t: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
