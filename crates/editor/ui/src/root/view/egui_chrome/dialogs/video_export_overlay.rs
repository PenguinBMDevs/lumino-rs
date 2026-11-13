//! 视频导出进度覆盖层：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::{tr, trf};
use lumino_ui_egui::chrome::{
    VideoExportOverlayModel, VideoOverlayCompleted, VideoOverlayError, VideoOverlayFinalizing,
    VideoOverlayProgress, VideoOverlayView,
};

use crate::Message;
use crate::root::Root;
use crate::state::root_state::{VideoExportDialogState, VideoExportOverlayState};

impl Root {
    /// 覆盖层模型；`None` = 当前没有覆盖层（设置面板照常显示）。
    pub(crate) fn video_export_overlay_model(&self) -> Option<VideoExportOverlayModel> {
        let state = &self.state.video_export_dialog;
        let progress = progress_block(state);
        let view = match &state.overlay {
            VideoExportOverlayState::None => return None,
            VideoExportOverlayState::Exporting => VideoOverlayView::Exporting(progress),
            VideoExportOverlayState::Finalizing => {
                VideoOverlayView::Finalizing(VideoOverlayFinalizing {
                    progress,
                    finalizing_line: tr!("正在完成编码...").to_string(),
                    hint: tr!("ffmpeg 正在封装文件，请稍候").to_string(),
                    force_finish: tr!("强制完成").to_string(),
                    force_hint: tr!("视频已可用，跳过等待").to_string(),
                })
            }
            VideoExportOverlayState::Completed {
                total_frames,
                elapsed_secs,
                avg_fps,
            } => {
                let duration = if state.fps > 0 {
                    *total_frames as f64 / state.fps as f64
                } else {
                    0.0
                };
                let speedup = if *elapsed_secs > 0.0 && duration > 0.0 {
                    duration / elapsed_secs
                } else {
                    0.0
                };
                VideoOverlayView::Completed(VideoOverlayCompleted {
                    heading: tr!("导出完成！").to_string(),
                    stats: vec![
                        trf!("总帧数: {n}", n = total_frames),
                        trf!("时长: {d}", d = format_duration(duration)),
                        trf!("总用时: {t}", t = format_duration(*elapsed_secs)),
                        trf!("平均速度: {fps} fps", fps = format!("{:.1}", avg_fps)),
                        trf!("倍率: {ratio}x 原速", ratio = format!("{:.1}", speedup)),
                    ],
                    confirm: tr!("确定").to_string(),
                })
            }
            VideoExportOverlayState::Error(err) => VideoOverlayView::Error(VideoOverlayError {
                heading: tr!("导出失败").to_string(),
                detail: err.clone(),
                confirm: tr!("确定").to_string(),
            }),
        };
        Some(VideoExportOverlayModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            view,
        })
    }

    /// egui 覆盖层动作 → 生产 `Message`。
    pub(crate) fn video_overlay_action_message(
        &self,
        action: &lumino_ui_egui::video_export_overlay::VideoOverlayAction,
    ) -> Option<Message> {
        use crate::message::VideoExportAction as Prod;
        use lumino_ui_egui::video_export_overlay::VideoOverlayAction as Action;
        Some(Message::VideoExport(match action {
            Action::CancelExport => Prod::CancelExport,
            Action::ForceFinish => Prod::ForceFinish,
            Action::Dismiss => Prod::DismissOverlay,
        }))
    }
}

/// 导出中 / 收尾共用的进度块。
fn progress_block(state: &VideoExportDialogState) -> VideoOverlayProgress {
    let fps = state.fps.max(1) as f64;
    let current_sec = state.current_frame as f64 / fps;
    let total_sec = state.total_frames as f64 / fps;
    let speedup = if current_sec > 0.0 && state.elapsed_secs > 0.0 {
        current_sec / state.elapsed_secs
    } else {
        0.0
    };
    VideoOverlayProgress {
        heading: tr!("视频导出中").to_string(),
        frames_line: trf!(
            "帧: {current} / {total}",
            current = state.current_frame,
            total = state.total_frames
        ),
        time_line: trf!(
            "时间: {elapsed} / {total}",
            elapsed = format_duration(current_sec),
            total = format_duration(total_sec)
        ),
        // 生产用 `progress_bar(0.0..=1.0, progress)`，进度值即 0..1。
        progress: state.progress as f32,
        percent_line: format!("{:.1}%", state.progress * 100.0),
        render_fps_line: trf!(
            "渲染速度: {fps} fps",
            fps = format!("{:.1}", state.render_fps)
        ),
        speed_line: trf!("速度: {speed}x 原速", speed = format!("{:.1}", speedup)),
        remaining_line: remaining_text(state),
    }
}

/// 剩余时间提示（与 旧版 `elapsed_remaining_text` 同口径）。
fn remaining_text(state: &VideoExportDialogState) -> String {
    let fps = state.render_fps.max(0.01);
    let remaining = state.total_frames.saturating_sub(state.current_frame) as f64 / fps;
    if state.current_frame == 0 || !remaining.is_finite() {
        return tr!("预计剩余: 计算中...").to_string();
    }
    trf!("预计剩余: {t}", t = format_duration(remaining))
}

/// 秒 → 「H:MM:SS」/「M:SS」（与 旧版 `format_duration` 同口径）。
fn format_duration(secs: f64) -> String {
    let total = secs.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
