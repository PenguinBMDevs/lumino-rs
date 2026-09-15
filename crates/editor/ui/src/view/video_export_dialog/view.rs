//! 视频导出对话框公开入口
//!
//! 渲染设置区各区块函数见 `render_settings` 子模块（由本文件拆分而来，
//! 保持单文件 ≤400 行红线）。

use iced_core::Length;
use iced_widget::{column, container, scrollable, space};

use super::layout::{buttons_section, midi_source_section, output_path_section, title_section};
use super::render_settings::render_settings_section;
use super::state::{VideoExportDialogState, VideoExportOverlayState};

// ── 公开入口 ────────────────────────────────────────────────

/// 渲染视频导出配置面板（侧边栏面板）
pub fn view_video_export_dialog<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let main_content = column![
        title_section(palette),
        space().height(16),
        midi_source_section(state, palette),
        space().height(16),
        render_settings_section(state, palette),
        space().height(16),
        output_path_section(state, palette),
        space().height(24),
        buttons_section(palette),
    ];

    let scrollable_content = scrollable(main_content)
        .width(Length::Fill)
        .height(Length::Fill);

    container(scrollable_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_t: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        })
        .into()
}

/// 渲染视频导出覆盖层（浮动 dialog 弹出样式）
pub fn view_video_export_overlay<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
) -> Option<crate::Element<'a>> {
    if matches!(state.overlay, VideoExportOverlayState::None) {
        return None;
    }
    let palette = theme.extended_palette();

    let content: crate::Element<'a> = match &state.overlay {
        VideoExportOverlayState::Exporting => {
            super::handlers::exporting_overlay(state, theme, palette)
        }
        VideoExportOverlayState::Finalizing => {
            super::handlers::finalizing_overlay(state, theme, palette)
        }
        VideoExportOverlayState::Completed {
            total_frames,
            elapsed_secs,
            avg_fps,
        } => super::handlers::completed_overlay(
            state,
            *total_frames,
            *elapsed_secs,
            *avg_fps,
            palette,
        ),
        VideoExportOverlayState::Error(err) => super::handlers::error_overlay(err.clone(), palette),
        VideoExportOverlayState::None => return None,
    };

    let full = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(20)
        .style(move |_t: &iced_core::Theme| container::Style {
            background: Some(palette.background.base.color.into()),
            ..Default::default()
        });

    Some(full.into())
}
