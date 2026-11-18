//! 独立 ffmpeg 写线程：有界帧队列消费，`write_frame` 背压不传导到帧生成线程。
//!
//! 核心并发逻辑（`enqueue_frame_or_cancel` / `writer_loop`）以纯函数实现，
//! 写入动作由闭包注入，便于单测；`FfmpegEncoder` 只在生产接线处出现。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lumino_core::trf;
use lumino_export::video::FfmpegEncoder;
use tokio::sync::mpsc::UnboundedSender;

use super::commands::send_export_error;
use super::frame::{EncodeTelemetry, ProgressMsg};

/// 有界入队轮询间隔（DEBT-07 #124：取消 200ms 生效，20ms 轮询足够）。
const ENQUEUE_POLL_INTERVAL: Duration = Duration::from_millis(20);
/// 写线程收帧超时（超时后复核取消标志）。
const FRAME_RECV_TIMEOUT: Duration = Duration::from_millis(100);

/// 入队结果。
#[derive(Debug, PartialEq, Eq)]
pub(super) enum EnqueueOutcome {
    /// 已入队。
    Enqueued,
    /// 队列满期间用户取消。
    Cancelled,
    /// 消费端已关闭。
    Closed,
}

/// 写线程退出原因。
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WriterOutcome {
    /// 队列排空且发送端关闭（正常收尾）。
    Drained,
    /// 用户取消。
    Cancelled,
    /// 写入失败（失败标志已置位）。
    Failed,
}

/// 有界入队：队列满时轮询取消标志，直到有空位 / 取消 / 通道关闭。
fn enqueue_frame_or_cancel<T>(
    tx: &SyncSender<T>,
    item: T,
    cancel: &AtomicBool,
    telemetry: &EncodeTelemetry,
    poll: Duration,
) -> EnqueueOutcome {
    let mut pending = item;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return EnqueueOutcome::Cancelled;
        }
        match tx.try_send(pending) {
            Ok(()) => {
                telemetry.queue_depth.fetch_add(1, Ordering::Relaxed);
                return EnqueueOutcome::Enqueued;
            }
            Err(TrySendError::Full(returned)) => {
                // 队列满：交还数据、短睡后重试；每轮复核取消（DEBT-07 #124）
                pending = returned;
                thread::sleep(poll);
            }
            Err(TrySendError::Disconnected(_)) => return EnqueueOutcome::Closed,
        }
    }
}

/// 写线程消费循环：收帧 → 写入闭包 → 累加遥测；取消/断连/失败时退出。
fn writer_loop<T, F, E>(
    frame_rx: &Receiver<T>,
    cancel: &AtomicBool,
    telemetry: &EncodeTelemetry,
    mut write: F,
    poll: Duration,
) -> WriterOutcome
where
    F: FnMut(T) -> Result<(), E>,
{
    loop {
        if cancel.load(Ordering::Relaxed) {
            return WriterOutcome::Cancelled;
        }
        match frame_rx.recv_timeout(poll) {
            Ok(item) => {
                // 饱和减，防止异常路径下计数下溢 panic
                let _ = telemetry.queue_depth.try_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |depth| Some(depth.saturating_sub(1)),
                );
                if write(item).is_err() {
                    telemetry.failed.store(true, Ordering::Relaxed);
                    return WriterOutcome::Failed;
                }
                telemetry.encoded_frames.fetch_add(1, Ordering::Relaxed);
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return WriterOutcome::Drained,
        }
    }
}

/// ffmpeg 写线程句柄：导出线程持有入队端，收尾时 drop 发送端并 join。
pub(super) struct EncoderThread {
    frame_tx: Option<SyncSender<Vec<u8>>>,
    handle: Option<JoinHandle<Result<(), String>>>,
    telemetry: Arc<EncodeTelemetry>,
}

impl EncoderThread {
    /// 启动写线程：创建有界队列，`write_frame` 在该线程内执行，背压与原导出线程隔离。
    ///
    /// 队列容量由 `capacity` 指定（已按内存预算封顶）。写线程失败时经 `progress_tx`
    /// 上报并置位 `telemetry.failed`；退出前必调 `encoder.finish()` 写文件头。
    pub(super) fn spawn(
        capacity: usize,
        mut encoder: FfmpegEncoder,
        recycle_tx: Sender<Vec<u8>>,
        progress_tx: UnboundedSender<ProgressMsg>,
        telemetry: Arc<EncodeTelemetry>,
        cancel: Arc<AtomicBool>,
    ) -> std::io::Result<Self> {
        let (frame_tx, frame_rx) = sync_channel(capacity);
        let writer_telemetry = Arc::clone(&telemetry);
        let handle = thread::Builder::new()
            .name("ffmpeg-writer".into())
            .spawn(move || {
                let outcome = writer_loop(
                    &frame_rx,
                    &cancel,
                    &telemetry,
                    |data| {
                        let started = Instant::now();
                        match encoder.write_frame(data) {
                            Ok(buf) => {
                                telemetry.encode_us_total.fetch_add(
                                    started.elapsed().as_micros() as u64,
                                    Ordering::Relaxed,
                                );
                                if recycle_tx.send(buf).is_err() {
                                    tracing::warn!("帧缓冲区归还失败：回收通道已关闭");
                                }
                                Ok(())
                            }
                            Err(e) => {
                                tracing::error!("写入视频帧失败: {e}");
                                send_export_error(&progress_tx, trf!("导出失败: {e}", e = e));
                                telemetry.failed.store(true, Ordering::Relaxed);
                                Err(e.to_string())
                            }
                        }
                    },
                    FRAME_RECV_TIMEOUT,
                );
                // 取消/排空/失败三条路径都必须 finish，确保 ffmpeg 写入文件头。
                encoder.finish().map_err(|e| e.to_string())?;
                match outcome {
                    WriterOutcome::Failed => Err("视频编码写入失败".to_string()),
                    _ => Ok(()),
                }
            })?;
        Ok(Self {
            frame_tx: Some(frame_tx),
            handle: Some(handle),
            telemetry: writer_telemetry,
        })
    }

    /// 入队一帧；队列满时轮询取消，供导出线程主循环调用。
    pub(super) fn enqueue(&self, data: Vec<u8>, cancel: &AtomicBool) -> EnqueueOutcome {
        let Some(tx) = self.frame_tx.as_ref() else {
            return EnqueueOutcome::Closed;
        };
        enqueue_frame_or_cancel(tx, data, cancel, &self.telemetry, ENQUEUE_POLL_INTERVAL)
    }

    /// 收尾：drop 发送端让写线程排空余帧后退出，join 并回报写入结果。
    pub(super) fn finish(mut self) -> Result<(), String> {
        self.frame_tx.take(); // drop → 写线程 recv Disconnected → 排空退出
        match self.handle.take() {
            Some(handle) => handle
                .join()
                .unwrap_or_else(|_| Err("ffmpeg 写线程 panic".to_string())),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn test_enqueue_success() {
        let (tx, rx) = sync_channel::<Vec<u8>>(2);
        let cancel = AtomicBool::new(false);
        let telemetry = EncodeTelemetry::default();

        let outcome = enqueue_frame_or_cancel(
            &tx,
            vec![1, 2, 3],
            &cancel,
            &telemetry,
            Duration::from_millis(1),
        );

        assert_eq!(outcome, EnqueueOutcome::Enqueued);
        assert_eq!(rx.recv().expect("应收到入队帧"), vec![1, 2, 3]);
        assert_eq!(telemetry.queue_depth.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_enqueue_full_then_cancel() {
        let (tx, _rx) = sync_channel::<Vec<u8>>(1); // 容量 1
        tx.try_send(vec![0]).expect("填满队列");
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_bg = Arc::clone(&cancel);
        let setter = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            cancel_bg.store(true, Ordering::Relaxed);
        });

        let telemetry = EncodeTelemetry::default();
        let start = Instant::now();
        let outcome =
            enqueue_frame_or_cancel(&tx, vec![9], &cancel, &telemetry, Duration::from_millis(5));
        setter.join().expect("取消设置线程应正常退出");

        assert_eq!(outcome, EnqueueOutcome::Cancelled);
        assert!(
            start.elapsed() < Duration::from_millis(300),
            "DEBT-07 #124：满队列下取消应在 200ms 量级生效（含 50ms 设置延迟 + 20ms 轮询）"
        );
        assert_eq!(telemetry.queue_depth.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_enqueue_detects_closed() {
        let (tx, rx) = sync_channel::<Vec<u8>>(1);
        drop(rx);
        let cancel = AtomicBool::new(false);
        let telemetry = EncodeTelemetry::default();

        let outcome =
            enqueue_frame_or_cancel(&tx, vec![1], &cancel, &telemetry, Duration::from_millis(1));

        assert_eq!(outcome, EnqueueOutcome::Closed);
    }

    #[test]
    fn test_writer_loop_drains_all() {
        let (tx, rx) = sync_channel::<u32>(4);
        for i in 0..3 {
            tx.try_send(i).expect("发送测试帧");
        }
        drop(tx); // 正常收尾：发送端关闭
        let cancel = AtomicBool::new(false);
        let telemetry = EncodeTelemetry::default();
        telemetry.queue_depth.store(3, Ordering::Relaxed);
        let written = Arc::new(Mutex::new(Vec::new()));
        let written_bg = Arc::clone(&written);

        let outcome = writer_loop(
            &rx,
            &cancel,
            &telemetry,
            |v| {
                written_bg.lock().expect("锁写入记录").push(v);
                Ok::<(), ()>(())
            },
            Duration::from_millis(5),
        );

        assert_eq!(outcome, WriterOutcome::Drained);
        assert_eq!(*written.lock().expect("读写入记录"), vec![0, 1, 2]);
        assert_eq!(telemetry.encoded_frames.load(Ordering::Relaxed), 3);
        assert_eq!(telemetry.queue_depth.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_writer_loop_cancel() {
        let (tx, rx) = sync_channel::<u32>(4);
        let _keep_alive = tx; // 发送端仍存活，模拟导出线程未关闭通道
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_bg = Arc::clone(&cancel);
        let setter = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            cancel_bg.store(true, Ordering::Relaxed);
        });
        let telemetry = EncodeTelemetry::default();
        let start = Instant::now();

        let outcome = writer_loop(
            &rx,
            &cancel,
            &telemetry,
            |_v| Ok::<(), ()>(()),
            Duration::from_millis(5),
        );
        setter.join().expect("取消设置线程应正常退出");

        assert_eq!(outcome, WriterOutcome::Cancelled);
        assert!(
            start.elapsed() < Duration::from_millis(300),
            "DEBT-07 #124：写线程取消退出应在 200ms 量级（含 50ms 设置延迟 + 轮询）"
        );
    }

    #[test]
    fn test_writer_loop_error_sets_failed() {
        let (tx, rx) = sync_channel::<u32>(4);
        tx.try_send(1).expect("发送测试帧");
        let cancel = AtomicBool::new(false);
        let telemetry = EncodeTelemetry::default();
        telemetry.queue_depth.store(1, Ordering::Relaxed);

        let outcome = writer_loop(
            &rx,
            &cancel,
            &telemetry,
            |_v| Err("模拟写入失败"),
            Duration::from_millis(5),
        );

        assert_eq!(outcome, WriterOutcome::Failed);
        assert!(telemetry.failed.load(Ordering::Relaxed));
        assert_eq!(telemetry.encoded_frames.load(Ordering::Relaxed), 0);
    }
}
