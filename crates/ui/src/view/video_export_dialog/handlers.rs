//! 视频导出覆盖层视图（各导出状态）
//!
//! 管理导出中、编码中、完成、失败四种状态的 UI 渲染。

use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, row, scrollable, space, text};

use crate::message::{Message, VideoExportAction};
use crate::view::widgets;

use super::helpers;
use super::layout::{preview_area, preview_area_empty};
use super::state::VideoExportDialogState;

/// 导出中的覆盖层
pub fn exporting_overlay<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let detail = helpers::render_progress_detail(state, theme);
    column![
        text("视频导出中")
            .size(16)
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        preview_area(state, palette),
        space().height(8),
        detail,
        space().height(12),
        button(text("取消导出").size(14))
            .on_press(Message::VideoExport(VideoExportAction::CancelExport))
            .padding([8, 32])
            .style(widgets::dialog_button_style(
                palette.danger.strong.color,
                palette.danger.base.color,
                Color::WHITE,
            )),
    ]
    .align_x(Alignment::Center)
    .into()
}

/// 编码中的覆盖层
pub fn finalizing_overlay<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let detail = helpers::render_progress_detail(state, theme);
    column![
        text("视频导出中")
            .size(16)
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        preview_area(state, palette),
        space().height(8),
        text("正在完成编码...")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        detail,
        space().height(4),
        text("ffmpeg 正在封装文件，请稍候")
            .size(12)
            .style(widgets::dialog_muted_text_style(palette)),
        space().height(8),
        row![
            button(text("强制完成").size(14))
                .on_press(Message::VideoExport(VideoExportAction::ForceFinish))
                .padding([6, 16])
                .style(widgets::dialog_button_style(
                    palette.background.strong.color,
                    palette.background.weak.color,
                    palette.background.neutral.text,
                )),
            space().width(8),
            text("视频已可用，跳过等待")
                .size(12)
                .style(widgets::dialog_muted_text_style(palette)),
        ]
        .align_y(Alignment::Center),
    ]
    .align_x(Alignment::Center)
    .into()
}

/// 导出完成的覆盖层
pub fn completed_overlay<'a>(
    state: &'a VideoExportDialogState,
    total_frames: u64,
    elapsed_secs: f64,
    avg_fps: f64,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let stats = ExportStats::new(state, total_frames, elapsed_secs, avg_fps);

    column![
        text("导出完成！")
            .size(16)
            .style(move |_t: &iced_core::Theme| text::Style {
                color: Some(palette.success.strong.color),
            }),
        space().height(12),
        preview_area_empty(palette),
        space().height(8),
        text(format!("总帧数: {total_frames}"))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        text(format!("时长: {}", helpers::format_duration(stats.duration)))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        text(format!("总用时: {}", helpers::format_duration(elapsed_secs)))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        text(format!("平均速度: {avg_fps:.1} fps"))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        text(format!("倍率: {:.1}x 原速", stats.speedup))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        space().height(16),
        button(text("确定").size(14))
            .on_press(Message::VideoExport(VideoExportAction::DismissOverlay))
            .padding([8, 32])
            .style(widgets::dialog_button_style(
                palette.primary.strong.color,
                palette.primary.base.color,
                Color::WHITE,
            )),
    ]
    .align_x(Alignment::Center)
    .into()
}

/// 导出失败的覆盖层
pub fn error_overlay<'a>(
    err: String,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("导出失败")
            .size(16)
            .style(move |_t: &iced_core::Theme| text::Style {
                color: Some(palette.danger.strong.color),
            }),
        space().height(12),
        container(
            scrollable(
                text(err)
                    .size(13)
                    .style(move |_t: &iced_core::Theme| text::Style {
                        color: Some(palette.danger.weak.color),
                    })
            )
            .height(Length::Fixed(200.0))
        )
        .width(Length::Fill),
        space().height(16),
        button(text("确定").size(14))
            .on_press(Message::VideoExport(VideoExportAction::DismissOverlay))
            .padding([8, 32])
            .style(widgets::dialog_button_style(
                palette.primary.strong.color,
                palette.primary.base.color,
                Color::WHITE,
            )),
    ]
    .align_x(Alignment::Center)
    .into()
}

// ── 内部辅助 ────────────────────────────────────────────────

/// 导出完成统计（从 `completed_overlay` 提取计算逻辑，降低函数行数）
struct ExportStats {
    duration: f64,
    speedup: f64,
}

impl ExportStats {
    fn new(
        state: &VideoExportDialogState,
        _total_frames: u64,
        elapsed_secs: f64,
        _avg_fps: f64,
    ) -> Self {
        let duration = if state.fps > 0 {
            _total_frames as f64 / state.fps as f64
        } else {
            0.0
        };
        let speedup = if elapsed_secs > 0.0 && duration > 0.0 {
            duration / elapsed_secs
        } else {
            0.0
        };
        Self { duration, speedup }
    }
}
