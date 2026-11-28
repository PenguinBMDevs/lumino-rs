//! 单帧合成 + 入队：把渲染线程读回的帧合成成最终画面并交给编码写线程。
//!
//! 原 `run_video_export_task` 内嵌 `process_frame` 闭包的捕获列表收敛为
//! `FrameEmitCtx`，闭包体本身抽成自由函数，便于单独审阅终止条件。

use lumino_core::tr;

use super::*;

/// 单帧合成/编码所需的捕获集合（原 `process_frame` 闭包的捕获列表）。
pub(super) struct FrameEmitCtx<'a> {
    pub(super) width: u32,
    pub(super) height: u32,
    /// GPU compute（瀑布流/MIDITrail）/ CPU 渲染（计数器）帧不含键盘/标尺，跳过合成
    pub(super) skip_chrome: bool,
    pub(super) keyboard_pixels: &'a [u8],
    pub(super) keyboard_w: u32,
    pub(super) keyboard_h: u32,
    pub(super) preview_tx: &'a UnboundedSender<(Vec<u8>, u32, u32)>,
    pub(super) last_preview_time: &'a mut Instant,
    pub(super) preview_sent: &'a mut bool,
    pub(super) encoder_thread: &'a mut EncoderThread,
    pub(super) cancel_flag: &'a Arc<AtomicBool>,
    pub(super) progress_tx: &'a UnboundedSender<ProgressMsg>,
}

/// 合成并编码单帧；返回 `(should_stop, stats)`。
pub(super) fn emit_frame(
    ctx: &mut FrameEmitCtx<'_>,
    message: VideoFrameMessage,
    frame_params: FrameParams,
) -> (bool, FrameStageStats) {
    let VideoFrameMessage { data, key_colors } = message;
    let params = if ctx.skip_chrome {
        FrameParams::default()
    } else {
        frame_params
    };
    let (kb_pixels, kb_w_use, kb_h_use): (&[u8], u32, u32) = if ctx.skip_chrome {
        (&[], 0, 0)
    } else {
        (ctx.keyboard_pixels, ctx.keyboard_w, ctx.keyboard_h)
    };
    // 钢琴卷帘键色由渲染线程 GPU 推导；None（空文档/聚合失败）按全 0
    const NO_KEY_COLORS: [u8; 1024] = [0u8; 1024];
    let key_colors = key_colors.as_ref().unwrap_or(&NO_KEY_COLORS);
    let (composed, stats) = composite_frame(CompositeFrameInput {
        data,
        params,
        preview_tx: ctx.preview_tx,
        last_preview_time: &mut *ctx.last_preview_time,
        preview_sent: &mut *ctx.preview_sent,
        width: ctx.width,
        height: ctx.height,
        keyboard_pixels: kb_pixels,
        kb_w: kb_w_use,
        kb_h: kb_h_use,
        key_colors,
    });
    let Some(data) = composed else {
        // 读回为空：跳过入队，继续后续帧（与旧路径丢弃该帧等价）
        return (false, stats);
    };
    match ctx.encoder_thread.enqueue(data, ctx.cancel_flag) {
        EnqueueOutcome::Enqueued => (false, stats),
        EnqueueOutcome::Cancelled => (true, stats),
        EnqueueOutcome::Closed => {
            // 取消置位恰在 pipeline 检查与 try_send 之间时，写线程可能已退出；
            // 此时按取消收尾，不误报「编码线程已退出」错误。
            if ctx.cancel_flag.load(Ordering::Relaxed) {
                tracing::info!("取消期间帧队列关闭，按取消收尾");
            } else {
                tracing::error!("帧队列消费端已关闭，终止帧生成");
                send_export_error(ctx.progress_tx, tr!("导出失败：编码线程已退出"));
            }
            (true, stats)
        }
    }
}
