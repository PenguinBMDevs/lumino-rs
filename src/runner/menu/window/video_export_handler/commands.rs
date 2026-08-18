//! 视频导出的命令发送、错误与收尾 helper。

use std::sync::mpsc::{Receiver, Sender};

use lumino_export::video::FfmpegEncoder;
use lumino_gfx::render_thread::{ControlCommand, RenderCommand};
use tokio::sync::mpsc::UnboundedSender;

/// 进度消息载荷：(文本, 进度 0..1, 总帧数, 平滑 FPS, 已用秒)
pub(super) type ProgressMsg = (String, f64, u64, f64, f64);

/// 发送导出失败进度消息（progress=-1 表示失败，UI 据此弹出错误）。
///
/// 收敛各处重复的 5 元组 `("导出失败: ..", -1.0, 0, 0.0, 0.0)` 发送。
pub(super) fn send_export_error(
    progress_tx: &UnboundedSender<ProgressMsg>,
    message: impl Into<String>,
) {
    let _ = progress_tx.send((message.into(), -1.0, 0, 0.0, 0.0));
}

/// 发送初始渲染命令：`StartVideoExport`。
///
/// 返回 `true` 表示发生通信错误、调用方应终止后台任务。
pub(super) fn send_initial_render_commands(
    cmd_sender: &Sender<RenderCommand>,
    width: u32,
    height: u32,
    frame_tx: Sender<Vec<u8>>,
    recycle_rx: Receiver<Vec<u8>>,
    progress_tx: &UnboundedSender<ProgressMsg>,
) -> bool {
    // 发送 StartVideoExport 命令，建立渲染线程对象池回收通道
    if cmd_sender
        .send(RenderCommand::Control(ControlCommand::StartVideoExport {
            width,
            height,
            frame_tx: lumino_gfx::render_thread::FrameSender(frame_tx),
            recycle_rx,
        }))
        .is_err()
    {
        tracing::error!("发送 StartVideoExport 命令失败");
        send_export_error(progress_tx, "导出失败：渲染线程通信错误");
        return true;
    }

    false
}

/// 收尾编码：根据是否取消发送最终进度，并调用 `finish()` 写入文件头。
pub(super) fn finalize_video_export(
    encoder: FfmpegEncoder,
    cancelled: bool,
    elapsed: f64,
    total_frames: u64,
    smoothed_fps: f64,
    progress_tx: &UnboundedSender<ProgressMsg>,
) {
    if !cancelled {
        let _ = progress_tx.send((
            "导出完成".to_string(),
            1.0,
            total_frames,
            smoothed_fps,
            elapsed,
        ));
    } else {
        let _ = progress_tx.send((
            "导出已取消".to_string(),
            1.0,
            total_frames,
            smoothed_fps,
            elapsed,
        ));
    }
    if let Err(e) = encoder.finish() {
        tracing::error!("FFmpeg 收尾失败: {e}");
        send_export_error(progress_tx, format!("导出失败: {e}"));
    }
}
