//! 帧流水线循环（预填充 → 主循环 → drain），内存/流式两条导出路径共用。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::UnboundedSender;

use super::super::video_export::cli_progress::CliProgressBar;
use super::commands::send_export_error;
use super::frame::{EncodeFrameQueue, FrameParams, FrameStageStats};

/// 进度消息载荷：(文本, 进度 0..1, 总帧数, 平滑 FPS, 已用秒)
type ProgressMsg = (String, f64, u64, f64, f64);

/// 帧流水线循环参数（预填充 → 主循环 → drain），内存/流式两条导出路径共用。
pub(super) struct FramePipeline<'a> {
    pub(super) total_frames: u64,
    pub(super) cancel_flag: &'a AtomicBool,
    pub(super) frame_rx: &'a Receiver<Vec<u8>>,
    pub(super) param_queue: &'a mut EncodeFrameQueue,
    pub(super) progress_tx: &'a UnboundedSender<ProgressMsg>,
    pub(super) render_bar: &'a mut CliProgressBar,
    /// 墙钟起点（用于进度消息中的"已用时间"）
    pub(super) start: Instant,
    /// 渲染进度映射：内存路径恒等，流式路径 `0.3 + raw * 0.7`（解析阶段占 0-0.3）
    pub(super) progress_map: fn(f64) -> f64,
}

/// 单次等待帧数据的上限（DEBT-07 #124）：到点轮询取消标志，取消 200ms 内生效。
const FRAME_WAIT_TIMEOUT: Duration = Duration::from_millis(100);

/// 帧等待失败原因。
#[derive(Debug)]
enum FrameWaitError {
    /// 用户取消/终止请求
    Cancelled,
    /// 生产者全部退出（渲染线程异常结束）
    Closed,
}

/// 等待一帧数据，带取消轮询（纯逻辑，可单测）。
///
/// 旧实现直接 `recv()` 阻塞：Runner 自持发送端（`frame_tx_waterfall`）时发送端
/// 永不 drop，渲染线程异常退出后 `recv()` 永久阻塞、取消标志形同虚设。
fn recv_frame_or_cancel(
    rx: &Receiver<Vec<u8>>,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Vec<u8>, FrameWaitError> {
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(FrameWaitError::Cancelled);
        }
        match rx.recv_timeout(timeout) {
            Ok(buf) => return Ok(buf),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(FrameWaitError::Closed);
            }
        }
    }
}

impl<'a> FramePipeline<'a> {
    /// 运行完整流水线：预填充 PIPELINE_DEPTH 帧 → 主循环（收帧→处理→补发）→ drain 余帧。
    ///
    /// - `enqueue(queue, frame_idx)`：入队并发送第 frame_idx 帧；返回 true 表示应终止。
    ///   queue 由本方法传入，闭包不得自行捕获 param_queue（避免双重可变借用）。
    /// - `process(frame_data, frame_params)`：处理单帧；返回 `(should_stop, stats)`。
    ///
    /// 返回 `(processed_frames, cancelled, smoothed_fps)`。
    pub(super) fn run<FE, FP>(&mut self, mut enqueue: FE, mut process: FP) -> (u64, bool, f64)
    where
        FE: FnMut(&mut EncodeFrameQueue, u64) -> bool,
        FP: FnMut(Vec<u8>, FrameParams) -> (bool, FrameStageStats),
    {
        const PIPELINE_DEPTH: usize = 4;
        let total_frames = self.total_frames;
        let mut processed_frames = 0u64;
        let mut cancelled = false;
        let mut next_frame_to_send = 0u64;

        let mut last_stat_time = Instant::now();
        let mut frames_since_stat = 0u64;
        let mut smoothed_fps = 0.0f64;
        let mut acc_recv_us = 0u64;
        let mut acc_composite_us = 0u64;
        let mut acc_preview_us = 0u64;
        let mut acc_encode_us = 0u64;
        let mut stat_frame_count = 0u64;

        // 预填充 inflight，让 GPU 从第一帧就进入流水线满载状态
        for _ in 0..PIPELINE_DEPTH.min(total_frames as usize) {
            if self.cancel_flag.load(Ordering::Relaxed) {
                tracing::info!("视频导出：用户取消，正在收尾...");
                cancelled = true;
                break;
            }
            if enqueue(self.param_queue, next_frame_to_send) {
                cancelled = true;
                break;
            }
            next_frame_to_send += 1;
        }

        // 主循环：每收到一帧就合成/编码，并立即补发下一帧命令
        while processed_frames < total_frames && !cancelled {
            if self.cancel_flag.load(Ordering::Relaxed) {
                tracing::info!("视频导出：用户取消，正在收尾...");
                cancelled = true;
                break;
            }

            let recv_start = Instant::now();
            // DEBT-07 #124：带超时轮询取消，不再对 recv() 永久阻塞
            let frame_data =
                match recv_frame_or_cancel(self.frame_rx, self.cancel_flag, FRAME_WAIT_TIMEOUT) {
                    Ok(buf) => buf,
                    Err(FrameWaitError::Cancelled) => {
                        tracing::info!("视频导出：用户取消（等待帧数据时），正在收尾...");
                        cancelled = true;
                        break;
                    }
                    Err(FrameWaitError::Closed) => {
                        tracing::error!("帧数据通道关闭（渲染线程异常退出）");
                        send_export_error(self.progress_tx, "导出失败：帧数据通道关闭");
                        cancelled = true;
                        break;
                    }
                };
            let recv_us = recv_start.elapsed().as_micros() as u64;

            // 默认值仅在 queue 与帧数据 FIFO 失步时出现（理论不发生），ppq 用 0 无实际影响
            let frame_params = self.param_queue.pop_front().unwrap_or_default();
            let (should_stop, stats) = process(frame_data, frame_params);

            acc_recv_us += recv_us;
            acc_composite_us += stats.composite_us;
            acc_preview_us += stats.preview_us;
            acc_encode_us += stats.encode_us;
            stat_frame_count += 1;

            if should_stop {
                cancelled = true;
                break;
            }

            processed_frames += 1;
            frames_since_stat += 1;

            // 维持流水线深度：每处理完一帧立即补发下一帧命令
            if next_frame_to_send < total_frames {
                if enqueue(self.param_queue, next_frame_to_send) {
                    cancelled = true;
                    break;
                }
                next_frame_to_send += 1;
            }

            // 阶段耗时打点：每 100ms 聚合输出一次
            if last_stat_time.elapsed() >= Duration::from_millis(100) && stat_frame_count > 0 {
                let elapsed = last_stat_time.elapsed().as_secs_f64();
                let fps = frames_since_stat as f64 / elapsed;
                smoothed_fps = if smoothed_fps == 0.0 {
                    fps
                } else {
                    smoothed_fps * 0.7 + fps * 0.3
                };
                let raw_progress = processed_frames as f64 / total_frames as f64;
                let progress = (self.progress_map)(raw_progress);
                let eta_secs = (total_frames - processed_frames) as f64 / smoothed_fps;
                let avg_recv = acc_recv_us / stat_frame_count;
                let avg_composite = acc_composite_us / stat_frame_count;
                let avg_preview = acc_preview_us / stat_frame_count;
                let avg_encode = acc_encode_us / stat_frame_count;
                self.render_bar.update(
                    raw_progress,
                    &format!(
                        "帧 {}/{} | FPS {:.0} | ETA {:.0}s | recv={} composite={} preview={} encode={}",
                        processed_frames,
                        total_frames,
                        smoothed_fps,
                        eta_secs,
                        avg_recv,
                        avg_composite,
                        avg_preview,
                        avg_encode,
                    ),
                );
                let _ = self.progress_tx.send((
                    format!(
                        "{:.0}% | FPS {:.0} | ETA {:.0}s",
                        progress * 100.0,
                        smoothed_fps,
                        eta_secs
                    ),
                    progress,
                    total_frames,
                    smoothed_fps,
                    // 真实已用时间（墙钟），供 UI 显示"已用时间"
                    self.start.elapsed().as_secs_f64(),
                ));
                last_stat_time = Instant::now();
                frames_since_stat = 0;
                acc_recv_us = 0;
                acc_composite_us = 0;
                acc_preview_us = 0;
                acc_encode_us = 0;
                stat_frame_count = 0;
            }
        }

        // drain 剩余 inflight 帧（DEBT-07 #124：同样带取消轮询，取消不挂起）
        while !self.param_queue.is_empty() && !cancelled {
            let drain_frame =
                match recv_frame_or_cancel(self.frame_rx, self.cancel_flag, FRAME_WAIT_TIMEOUT) {
                    Ok(buf) => buf,
                    Err(FrameWaitError::Cancelled) => {
                        tracing::info!("视频导出：用户取消（drain 阶段），正在收尾...");
                        cancelled = true;
                        break;
                    }
                    Err(FrameWaitError::Closed) => {
                        tracing::error!("drain 阶段帧数据通道关闭");
                        cancelled = true;
                        break;
                    }
                };

            let drain_params = self.param_queue.pop_front().unwrap_or_default();
            let (should_stop, _stats) = process(drain_frame, drain_params);

            if should_stop {
                cancelled = true;
                break;
            }
            processed_frames += 1;
        }

        (processed_frames, cancelled, smoothed_fps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DEBT-07 #124：等待帧数据期间取消应即时返回（生产者仍持有发送端）。
    #[test]
    fn recv_frame_or_cancel_returns_on_cancel() {
        let (_tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let cancel = AtomicBool::new(true);
        let result = recv_frame_or_cancel(&rx, &cancel, Duration::from_millis(50));
        assert!(matches!(result, Err(FrameWaitError::Cancelled)));
    }

    /// 生产者全部退出 → Closed（旧实现会永久阻塞）。
    #[test]
    fn recv_frame_or_cancel_detects_closed_channel() {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        drop(tx);
        let cancel = AtomicBool::new(false);
        let result = recv_frame_or_cancel(&rx, &cancel, Duration::from_millis(50));
        assert!(matches!(result, Err(FrameWaitError::Closed)));
    }

    /// 数据正常到达 → Ok；取消标志为 false 时不误伤。
    #[test]
    fn recv_frame_or_cancel_returns_data() {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        tx.send(vec![1, 2, 3]).expect("发送测试帧");
        let cancel = AtomicBool::new(false);
        let result = recv_frame_or_cancel(&rx, &cancel, Duration::from_millis(50));
        assert_eq!(result.expect("应收到数据"), vec![1, 2, 3]);
    }
}
