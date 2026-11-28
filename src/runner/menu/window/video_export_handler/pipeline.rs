//! 帧流水线循环（预填充 → 主循环 → drain），内存/流式两条导出路径共用。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use lumino_core::tr;
use lumino_gfx::render_thread::VideoFrameMessage;
use tokio::sync::mpsc::UnboundedSender;

use super::super::video_export::cli_progress::CliProgressBar;
use super::commands::send_export_error;
use super::frame::{
    EncodeFrameQueue, EncodeTelemetry, FrameParams, FrameStageStats, PIPELINE_DEPTH, ProgressMsg,
};

/// 帧流水线循环参数（预填充 → 主循环 → drain），内存/流式两条导出路径共用。
pub(super) struct FramePipeline<'a> {
    pub(super) total_frames: u64,
    /// 诊断用帧数上限（`LUMINO_BENCH_FRAMES`，Tracy 基线抓固定窗口）；
    /// `None` = 导出完整视频。上限同时收敛进度分母，收尾按「完成」处理。
    pub(super) frame_limit: Option<u64>,
    pub(super) cancel_flag: &'a AtomicBool,
    pub(super) frame_rx: &'a Receiver<VideoFrameMessage>,
    pub(super) param_queue: &'a mut EncodeFrameQueue,
    pub(super) progress_tx: &'a UnboundedSender<ProgressMsg>,
    pub(super) render_bar: &'a mut CliProgressBar,
    /// 墙钟起点（用于进度消息中的"已用时间"）
    pub(super) start: Instant,
    /// 渲染进度映射：内存路径恒等，流式路径 `0.3 + raw * 0.7`（解析阶段占 0-0.3）
    pub(super) progress_map: fn(f64) -> f64,
    /// 跨线程编码遥测：读队列深度/编码 FPS，检测写线程失败。
    pub(super) telemetry: &'a EncodeTelemetry,
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
fn recv_frame_or_cancel<T>(
    rx: &Receiver<T>,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<T, FrameWaitError> {
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

/// 三段打点窗口（帧生成 FPS / 编码消费 FPS / 队列深度），每 100ms 结算并清零。
///
/// 字段数 8：纯累加器 DTO，`smoothed_fps` 跨窗口保留供 ETA 与最终返回值使用。
struct StatsWindow {
    /// 上次结算时刻
    last_stat_time: Instant,
    /// 距上次结算已处理帧数
    frames_since_stat: u64,
    /// 平滑后的帧生成 FPS
    smoothed_fps: f64,
    /// 窗口内收帧耗时累加（微秒）
    acc_recv_us: u64,
    /// 窗口内合成耗时累加（微秒）
    acc_composite_us: u64,
    /// 窗口内预览耗时累加（微秒）
    acc_preview_us: u64,
    /// 窗口内结算帧数
    stat_frame_count: u64,
    /// 编码消费 FPS 基准（写线程跨线程累加，按窗口增量计算）
    last_encoded_frames: u64,
}

impl StatsWindow {
    /// 距上次结算超过 100ms 时输出进度条 / 进度消息 / Tracy 曲线，然后清零窗口。
    ///
    /// 三段打点：每 100ms 聚合输出一次
    /// ① 帧生成 FPS = 导出线程成功入队帧速率（队列不满时 = 纯帧生成速度）
    /// ② 编码消费 FPS = 写线程 write_frame 完成速率（按窗口增量）
    /// ③ 队列深度 = 当前有界队列积压帧数
    ///
    /// 窗口字段先取到局部变量、结算完再写回，语句顺序与原实现保持一致。
    fn report_if_due(
        &mut self,
        pipeline: &mut FramePipeline<'_>,
        processed_frames: u64,
        total_frames: u64,
    ) {
        let mut last_stat_time = self.last_stat_time;
        let mut frames_since_stat = self.frames_since_stat;
        let mut smoothed_fps = self.smoothed_fps;
        let mut acc_recv_us = self.acc_recv_us;
        let mut acc_composite_us = self.acc_composite_us;
        let mut acc_preview_us = self.acc_preview_us;
        let mut stat_frame_count = self.stat_frame_count;
        let mut last_encoded_frames = self.last_encoded_frames;

        if last_stat_time.elapsed() >= Duration::from_millis(100) && stat_frame_count > 0 {
            let elapsed = last_stat_time.elapsed().as_secs_f64();
            let gen_fps = frames_since_stat as f64 / elapsed;
            smoothed_fps = if smoothed_fps == 0.0 {
                gen_fps
            } else {
                smoothed_fps * 0.7 + gen_fps * 0.3
            };
            let encoded_now = pipeline.telemetry.encoded_frames.load(Ordering::Relaxed);
            let encode_fps = encoded_now.saturating_sub(last_encoded_frames) as f64 / elapsed;
            last_encoded_frames = encoded_now;
            let queue_depth = pipeline.telemetry.queue_depth.load(Ordering::Relaxed);

            // Tracy 曲线：三段解耦后的实时速率/积压（与日志打点同一口径）
            crate::profiling::tracy_plot!("app.gen_fps", gen_fps);
            crate::profiling::tracy_plot!("app.encode_fps", encode_fps);
            crate::profiling::tracy_plot!("app.queue_depth", queue_depth);

            let raw_progress = processed_frames as f64 / total_frames as f64;
            let progress = (pipeline.progress_map)(raw_progress);
            let eta_secs = (total_frames - processed_frames) as f64 / smoothed_fps;
            let avg_recv = acc_recv_us / stat_frame_count;
            let avg_composite = acc_composite_us / stat_frame_count;
            let avg_preview = acc_preview_us / stat_frame_count;
            pipeline.render_bar.update(
                raw_progress,
                &format!(
                    "帧 {}/{} | 生成FPS {:.0} | 编码FPS {:.0} | 队列 {} | ETA {:.0}s | recv={} 合成={} 预览={}",
                    processed_frames, total_frames, smoothed_fps, encode_fps,
                    queue_depth, eta_secs, avg_recv, avg_composite, avg_preview,
                ),
            );
            let _ = pipeline.progress_tx.send((
                format!(
                    "{:.0}% | 生成FPS {:.0} | ETA {:.0}s",
                    progress * 100.0,
                    smoothed_fps,
                    eta_secs
                ),
                progress,
                total_frames,
                smoothed_fps,
                // 真实已用时间（墙钟），供 UI 显示"已用时间"
                pipeline.start.elapsed().as_secs_f64(),
            ));
            last_stat_time = Instant::now();
            frames_since_stat = 0;
            acc_recv_us = 0;
            acc_composite_us = 0;
            acc_preview_us = 0;
            stat_frame_count = 0;
        }

        self.last_stat_time = last_stat_time;
        self.frames_since_stat = frames_since_stat;
        self.smoothed_fps = smoothed_fps;
        self.acc_recv_us = acc_recv_us;
        self.acc_composite_us = acc_composite_us;
        self.acc_preview_us = acc_preview_us;
        self.stat_frame_count = stat_frame_count;
        self.last_encoded_frames = last_encoded_frames;
    }
}

impl<'a> FramePipeline<'a> {
    /// 预填充 `PIPELINE_DEPTH` 帧，让 GPU 从第一帧就进入流水线满载状态。
    ///
    /// 返回（下一条待发帧号，是否已被取消）。
    fn prefill_inflight<FE>(&mut self, enqueue: &mut FE, total_frames: u64) -> (u64, bool)
    where
        FE: FnMut(&mut EncodeFrameQueue, u64) -> bool,
    {
        let mut next_frame_to_send = 0u64;
        let mut cancelled = false;
        for _ in 0..PIPELINE_DEPTH.min(total_frames as usize) {
            if self.cancel_flag.load(Ordering::Relaxed) {
                tracing::info!("视频导出：用户取消，正在收尾...");
                cancelled = true;
                break;
            }
            // 先绑变量再判断：zone 宏在默认构建（无 tracy）展开为块表达式，
            // 直接放进 if 条件会触发 clippy::blocks_in_conditions。
            let enqueue_cancelled = crate::profiling::tracy_zone!("app.pipeline.prefill", {
                enqueue(self.param_queue, next_frame_to_send)
            });
            if enqueue_cancelled {
                cancelled = true;
                break;
            }
            next_frame_to_send += 1;
        }
        (next_frame_to_send, cancelled)
    }

    /// 主循环单帧：取消 / 写线程失败检查 → 带超时收帧 → 处理。
    ///
    /// 返回 `None` 表示应结束主循环（日志已在此输出）；
    /// `Some((收帧耗时微秒, 帧统计, should_stop))` 表示已处理一帧。
    fn process_next<FP>(&mut self, process: &mut FP) -> Option<(u64, FrameStageStats, bool)>
    where
        FP: FnMut(VideoFrameMessage, FrameParams) -> (bool, FrameStageStats),
    {
        if self.cancel_flag.load(Ordering::Relaxed) {
            tracing::info!("视频导出：用户取消，正在收尾...");
            return None;
        }
        // 写线程 write_frame 失败：经 telemetry 传播到导出线程，立即终止帧生成
        if self.telemetry.failed.load(Ordering::Relaxed) {
            tracing::error!("视频导出：ffmpeg 写线程失败，终止帧生成");
            return None;
        }

        let recv_start = Instant::now();
        // DEBT-07 #124：带超时轮询取消，不再对 recv() 永久阻塞
        let message =
            match recv_frame_or_cancel(self.frame_rx, self.cancel_flag, FRAME_WAIT_TIMEOUT) {
                Ok(message) => message,
                Err(FrameWaitError::Cancelled) => {
                    tracing::info!("视频导出：用户取消（等待帧数据时），正在收尾...");
                    return None;
                }
                Err(FrameWaitError::Closed) => {
                    tracing::error!("帧数据通道关闭（渲染线程异常退出）");
                    send_export_error(self.progress_tx, tr!("导出失败：帧数据通道关闭"));
                    return None;
                }
            };
        let recv_us = recv_start.elapsed().as_micros() as u64;

        // 默认值仅在 queue 与帧数据 FIFO 失步时出现（理论不发生），ppq 用 0 无实际影响
        let frame_params = self.param_queue.pop_front().unwrap_or_default();
        let (should_stop, stats) = crate::profiling::tracy_zone!("app.pipeline.process", {
            process(message, frame_params)
        });

        Some((recv_us, stats, should_stop))
    }

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
        FP: FnMut(VideoFrameMessage, FrameParams) -> (bool, FrameStageStats),
    {
        // 诊断帧上限与总帧数取小：预填充/补发/进度分母全部按有效总数收敛
        let total_frames = self.total_frames.min(self.frame_limit.unwrap_or(u64::MAX));
        let mut processed_frames = 0u64;
        let mut stats_window = StatsWindow {
            last_stat_time: Instant::now(),
            frames_since_stat: 0u64,
            smoothed_fps: 0.0f64,
            acc_recv_us: 0u64,
            acc_composite_us: 0u64,
            acc_preview_us: 0u64,
            stat_frame_count: 0u64,
            // 编码消费 FPS 基准（写线程跨线程累加，按窗口增量计算）
            last_encoded_frames: self.telemetry.encoded_frames.load(Ordering::Relaxed),
        };

        // 预填充 inflight，让 GPU 从第一帧就进入流水线满载状态
        let (mut next_frame_to_send, mut cancelled) =
            self.prefill_inflight(&mut enqueue, total_frames);

        // 主循环：每收到一帧就合成/编码，并立即补发下一帧命令
        while processed_frames < total_frames && !cancelled {
            let Some((recv_us, stats, should_stop)) = self.process_next(&mut process) else {
                cancelled = true;
                break;
            };

            stats_window.acc_recv_us += recv_us;
            stats_window.acc_composite_us += stats.composite_us;
            stats_window.acc_preview_us += stats.preview_us;
            stats_window.stat_frame_count += 1;

            if should_stop {
                cancelled = true;
                break;
            }

            processed_frames += 1;
            stats_window.frames_since_stat += 1;

            // 维持流水线深度：每处理完一帧立即补发下一帧命令
            if next_frame_to_send < total_frames {
                let enqueue_cancelled = crate::profiling::tracy_zone!("app.pipeline.enqueue", {
                    enqueue(self.param_queue, next_frame_to_send)
                });
                if enqueue_cancelled {
                    cancelled = true;
                    break;
                }
                next_frame_to_send += 1;
            }

            stats_window.report_if_due(self, processed_frames, total_frames);
        }

        let (processed_frames, cancelled) =
            self.drain_inflight(&mut process, processed_frames, cancelled);

        (processed_frames, cancelled, stats_window.smoothed_fps)
    }

    /// drain 剩余 inflight 帧（DEBT-07 #124：同样带取消轮询，取消不挂起）。
    ///
    /// 返回（更新后的已处理帧数，是否已被取消）。
    fn drain_inflight<FP>(
        &mut self,
        process: &mut FP,
        mut processed_frames: u64,
        mut cancelled: bool,
    ) -> (u64, bool)
    where
        FP: FnMut(VideoFrameMessage, FrameParams) -> (bool, FrameStageStats),
    {
        while !self.param_queue.is_empty() && !cancelled {
            if self.telemetry.failed.load(Ordering::Relaxed) {
                tracing::error!("视频导出：写线程失败，drain 阶段终止");
                cancelled = true;
                break;
            }
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
        (processed_frames, cancelled)
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
