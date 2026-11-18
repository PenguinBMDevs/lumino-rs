//! 视频导出的命令发送、错误与收尾 helper。

use std::sync::mpsc::{Receiver, Sender};

use lumino_core::{tr, trf};
use lumino_gfx::render_thread::{ControlCommand, RenderCommand, VideoFrameMessage};
use tokio::sync::mpsc::UnboundedSender;

use super::encoder_thread::EncoderThread;
use super::frame::ProgressMsg;

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
    frame_tx: Sender<VideoFrameMessage>,
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
        send_export_error(progress_tx, tr!("导出失败：渲染线程通信错误"));
        return true;
    }

    false
}

/// 发送收尾渲染命令：`FinishVideoExport`（渲染线程释放导出常驻）。
///
/// 成功/取消双路径必达（由 `finalize_video_export` 调用）：常驻不清，
/// 下次导出前显存始终被旧数据占着（24M 文档约 700MB+）。best-effort：
/// 渲染线程已死则无事可做，记 warn 不阻断 ffmpeg 收尾。
pub(super) fn send_finish_render_command(cmd_sender: &Sender<RenderCommand>) {
    if cmd_sender
        .send(RenderCommand::Control(ControlCommand::FinishVideoExport))
        .is_err()
    {
        tracing::warn!("发送 FinishVideoExport 命令失败（渲染线程可能已退出）");
    }
}

/// 收尾编码：根据是否取消发送最终进度，并调用 `finish()` 写入文件头。
///
/// `tdr_cancelled`（R5-b）：取消由重连流水线（GPU 设备丢失）置位时为 true——
/// 按「失败」上报 UI（progress=-1 → 弹错误提示），而非静默「已取消」。
#[allow(clippy::too_many_arguments)] // 内存/流式两条导出路径共用的收尾接口，保持显式传递
pub(super) fn finalize_video_export(
    cmd_sender: &Sender<RenderCommand>,
    encoder_thread: EncoderThread,
    cancelled: bool,
    tdr_cancelled: bool,
    encode_failed: bool,
    elapsed: f64,
    total_frames: u64,
    smoothed_fps: f64,
    progress_tx: &UnboundedSender<ProgressMsg>,
) {
    // 先放显存：渲染线程收到后即释常驻，与 ffmpeg 收尾并发，不 blocking 编码。
    send_finish_render_command(cmd_sender);

    // 先 join 写线程拿到最终写入结果，再单点决策最终进度消息：
    // 避免「先发完成、后发失败」自相矛盾，也避免通用文案覆盖 write 的详细错误。
    let finish_err = encoder_thread.finish().err();

    if encode_failed {
        // 写线程已上报含 {e} 详细原因的写入错误，此处只记日志，不再重复发送。
        tracing::error!("视频导出因编码写入失败中断（详细错误已由写线程上报）");
        if let Some(e) = &finish_err {
            tracing::error!("ffmpeg 收尾次生失败: {e}");
        }
    } else if let Some(e) = &finish_err {
        // 写入阶段全部成功，但 ffmpeg finish 非零退出/超时——必须明确报错而非「完成」。
        tracing::error!("FFmpeg 收尾失败: {e}");
        send_export_error(progress_tx, trf!("导出失败: {e}", e = e));
    } else if !cancelled {
        let _ = progress_tx.send((
            tr!("导出完成").to_string(),
            1.0,
            total_frames,
            smoothed_fps,
            elapsed,
        ));
    } else if tdr_cancelled {
        // R5-b（用户实测 2026-10-17）：TDR 打断导出必须给用户明确错误——
        // 旧实现归入「已取消」（progress=1.0）导致无任何提示。
        tracing::error!("视频导出因 GPU 设备丢失（TDR）中断，按失败上报 UI");
        send_export_error(
            progress_tx,
            tr!("导出失败：GPU 设备丢失（驱动超时恢复）——导出已中断，渲染已自动恢复，可直接重试"),
        );
    } else {
        let _ = progress_tx.send((
            tr!("导出已取消").to_string(),
            1.0,
            total_frames,
            smoothed_fps,
            elapsed,
        ));
    }
}
