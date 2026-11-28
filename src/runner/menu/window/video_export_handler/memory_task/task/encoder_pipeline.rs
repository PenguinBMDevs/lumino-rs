//! 编码器装配：帧数据通道、FFmpeg 编码器、独立写线程与遥测。
//!
//! 内存/流式两条导出路径都在此建立同一条「导出线程 → 写线程」流水线；
//! 写线程的队列容量由 `FrameQueueConfig` 按时间与内存双预算封顶。

use lumino_core::trf;

use super::*;

/// 编码侧三件套：写线程句柄、遥测、帧接收端 + CPU 直出用发送端。
///
/// `StartVideoExport` 消费的发送端与缓冲回收端由 `spawn` 一并交出
/// （见返回值的后两项），本结构只保留渲染期间仍要用的部分。
pub(super) struct EncoderPipeline {
    pub(super) thread: EncoderThread,
    pub(super) telemetry: Arc<EncodeTelemetry>,
    pub(super) frame_rx: std::sync::mpsc::Receiver<VideoFrameMessage>,
    /// 瀑布流 CPU 路径需在 `enqueue_frame` 中保留发送能力。
    pub(super) frame_tx_waterfall: std::sync::mpsc::Sender<VideoFrameMessage>,
}

impl EncoderPipeline {
    /// 创建通道、编码器与写线程；任一失败已上报 `progress_tx` 并返回 `None`。
    ///
    /// 返回 `(pipeline, frame_tx, recycle_rx)`：后两项要交给渲染线程的
    /// `StartVideoExport` 命令，pipeline 内的 `frame_tx_waterfall` 是它的副本。
    pub(super) fn spawn(
        config: &VideoExportConfig,
        input_pix_fmt: &'static str,
        geometry: &FrameGeometry,
        cancel_flag: &Arc<AtomicBool>,
        progress_tx: &UnboundedSender<ProgressMsg>,
    ) -> Option<(
        Self,
        std::sync::mpsc::Sender<VideoFrameMessage>,
        std::sync::mpsc::Receiver<Vec<u8>>,
    )> {
        // 创建帧数据通道与回收通道（钢琴卷帘键色随帧消息同序传递）
        let (frame_tx, frame_rx) = channel::<VideoFrameMessage>();
        let (recycle_tx, recycle_rx) = channel::<Vec<u8>>();

        // 创建 FFmpeg 编码器；write_frame 将移至独立写线程执行
        let encoder = match FfmpegEncoder::new(config, input_pix_fmt) {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("FFmpeg 创建失败: {e}");
                send_export_error(progress_tx, trf!("导出失败: {e}", e = e));
                return None;
            }
        };

        // 有界帧队列容量：时间预算 ∩ 内存预算封顶，吸收短期背压而不 OOM。
        let queue_cfg = FrameQueueConfig {
            width: geometry.width,
            height: geometry.height,
            fps: geometry.fps_f64,
            buffer_seconds: QUEUE_BUFFER_SECONDS,
            max_bytes: QUEUE_MAX_BYTES,
            min_frames: PIPELINE_DEPTH,
        };
        let telemetry = Arc::new(EncodeTelemetry::default());
        let thread = match EncoderThread::spawn(
            queue_cfg.capacity(),
            encoder,
            recycle_tx,
            progress_tx.clone(),
            Arc::clone(&telemetry),
            Arc::clone(cancel_flag),
        ) {
            Ok(thread) => thread,
            Err(e) => {
                tracing::error!("启动 ffmpeg 写线程失败: {e}");
                send_export_error(progress_tx, trf!("导出失败：写线程启动失败: {e}", e = e));
                return None;
            }
        };
        tracing::info!(
            "帧队列解耦就绪: 容量={} 帧 ({}x{}, {}fps, 缓存 {:.1}s)",
            queue_cfg.capacity(),
            geometry.width,
            geometry.height,
            geometry.fps_f64,
            QUEUE_BUFFER_SECONDS
        );

        // clone frame_tx：交给渲染线程的发送端会被消费，瀑布流 CPU 路径需保留发送能力。
        let frame_tx_waterfall = frame_tx.clone();
        Some((
            Self {
                thread,
                telemetry,
                frame_rx,
                frame_tx_waterfall,
            },
            frame_tx,
            recycle_rx,
        ))
    }
}
