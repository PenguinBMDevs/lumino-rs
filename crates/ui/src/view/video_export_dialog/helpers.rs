//! 视频导出对话框的渲染帮助函数
//!
//! 从 `video_export_dialog.rs` 中提取，降低主文件单文件规模。

use crate::state::root_state::VideoExportDialogState;
use iced_widget::{column, progress_bar, space, text};

/// 渲染进度详情（帧数/时间/进度条/速度/倍率/已用剩余）
pub fn render_progress_detail<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();
    let weak_text = move |_t: &iced_core::Theme| text::Style {
        color: Some(palette.background.weak.text),
    };

    let fps = state.fps.max(1) as f64;
    let current_sec = state.current_frame as f64 / fps;
    let total_sec = state.total_frames as f64 / fps;
    let speedup = if current_sec > 0.0 && state.render_fps > 0.0 {
        current_sec / (state.current_frame as f64 / state.render_fps)
    } else {
        0.0
    };

    let elapsed_str = if state.current_frame > 0 && state.render_fps > 0.0 {
        let elapsed = state.current_frame as f64 / state.render_fps;
        let remaining = (state.total_frames - state.current_frame) as f64 / state.render_fps;
        format!(
            "已用: {} / 剩余: {}",
            format_duration(elapsed),
            format_duration(remaining)
        )
    } else {
        format!("已用: {}", format_duration(0.0))
    };

    column![
        text(format!(
            "帧: {} / {}",
            state.current_frame, state.total_frames
        ))
        .size(13)
        .style(weak_text),
        text(format!(
            "时间: {} / {}",
            format_duration(current_sec),
            format_duration(total_sec)
        ))
        .size(13)
        .style(weak_text),
        space().height(4),
        progress_bar(0.0..=1.0, state.progress as f32),
        space().height(4),
        text(format!("{:.1}%", state.progress * 100.0))
            .size(12)
            .style(weak_text),
        space().height(8),
        text(format!("渲染速度: {:.1} fps", state.render_fps))
            .size(13)
            .style(weak_text),
        text(format!("速度: {:.1}x 原速", speedup))
            .size(13)
            .style(weak_text),
        text(elapsed_str).size(13).style(weak_text),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 格式化时长（秒 → "M:SSS.S" 或 "H:MM:SSS.S"）
pub fn format_duration(secs: f64) -> String {
    if secs <= 0.0 {
        return "0:00.0".to_string();
    }
    let total_tenths = (secs * 10.0) as u64;
    let tenths = total_tenths % 10;
    let total_secs = total_tenths / 10;
    let s = total_secs % 60;
    let total_mins = total_secs / 60;
    let m = total_mins % 60;
    let h = total_mins / 60;
    if h > 0 {
        format!("{}:{}:{:02}.{}", h, m, s, tenths)
    } else {
        format!("{}:{:02}.{}", m, s, tenths)
    }
}

/// 返回当前平台可用的加速后端列表
pub fn available_backends() -> Vec<String> {
    let mut list = vec!["Software (CPU)".to_string()];
    #[cfg(target_os = "macos")]
    list.push("VideoToolbox (macOS)".to_string());
    #[cfg(target_os = "windows")]
    {
        list.push("NVENC (NVIDIA)".to_string());
        list.push("AMF (AMD)".to_string());
        list.push("QSV (Intel)".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        list.push("NVENC (NVIDIA)".to_string());
        list.push("QSV (Intel)".to_string());
        list.push("VAAPI (Linux)".to_string());
    }
    list
}
