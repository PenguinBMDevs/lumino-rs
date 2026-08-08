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
    let speedup = if current_sec > 0.0 && state.elapsed_secs > 0.0 {
        current_sec / state.elapsed_secs
    } else {
        0.0
    };

    let elapsed_str = elapsed_remaining_text(
        state.elapsed_secs,
        state.current_frame,
        state.total_frames,
        state.render_fps,
    );

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
    let secs = total_secs % 60;
    let total_mins = total_secs / 60;
    let mins = total_mins % 60;
    let hours = total_mins / 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}.{}", hours, mins, secs, tenths)
    } else {
        format!("{}:{:02}.{}", mins, secs, tenths)
    }
}

/// 生成"已用: X / 剩余: Y"文本。
///
/// 已用时间使用导出线程测量的墙钟真实时间（`elapsed_secs`）；
/// 剩余时间为基于渲染速度的估算（瞬时速度，仅作预测参考）。
pub fn elapsed_remaining_text(
    elapsed_secs: f64,
    current_frame: u64,
    total_frames: u64,
    render_fps: f64,
) -> String {
    if elapsed_secs > 0.0 && render_fps > 0.0 {
        let remaining = total_frames.saturating_sub(current_frame) as f64 / render_fps;
        format!(
            "已用: {} / 剩余: {}",
            format_duration(elapsed_secs),
            format_duration(remaining)
        )
    } else {
        format!("已用: {}", format_duration(elapsed_secs))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_basic() {
        assert_eq!(format_duration(0.0), "0:00.0");
        assert_eq!(format_duration(-1.0), "0:00.0");
        assert_eq!(format_duration(1.5), "0:01.5");
        assert_eq!(format_duration(59.9), "0:59.9");
        assert_eq!(format_duration(60.0), "1:00.0");
        assert_eq!(format_duration(3599.9), "59:59.9");
        assert_eq!(format_duration(3600.0), "1:00:00.0");
        assert_eq!(format_duration(3661.25), "1:01:01.2");
    }

    /// 已用时间必须使用真实 elapsed_secs，不能由 current_frame/render_fps 反推。
    #[test]
    fn test_elapsed_text_uses_real_wall_clock() {
        // 速度波动场景：真实已用 42.5s，但瞬时 render_fps 高达 200（当前帧 6000 帧）
        let text = elapsed_remaining_text(42.5, 6000, 12000, 200.0);
        assert!(text.starts_with("已用: 0:42.5"), "实际: {text}");
        // 剩余 = (12000-6000)/200 = 30s
        assert!(text.ends_with("剩余: 0:30.0"), "实际: {text}");
    }

    /// elapsed 为 0 时（初始阶段）不显示剩余，避免瞬时速度误导。
    #[test]
    fn test_elapsed_text_zero_fallback() {
        let text = elapsed_remaining_text(0.0, 0, 1000, 60.0);
        assert_eq!(text, "已用: 0:00.0");
    }

    /// render_fps 为 0 时剩余估算不可用，只显示已用。
    #[test]
    fn test_elapsed_text_no_render_fps() {
        let text = elapsed_remaining_text(10.0, 500, 1000, 0.0);
        assert_eq!(text, "已用: 0:10.0");
    }
}
