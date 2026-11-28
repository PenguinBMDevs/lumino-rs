use super::*;

mod cpu;
mod gpu;

/// 内存模式单帧入队（原 `run_video_export_task` 内嵌 `enqueue_frame` 闭包抽出的自由函数）。
///
/// 计算当前 tick 的按键高亮色与标尺偏移并入队 `FrameParams`；CPU 渲染模式
/// （瀑布流/计数器/数据曲线）在此直接生成帧数据并送入编码通道，跳过 GPU 路径。
/// 返回 `true` 表示应终止渲染循环（配置缺失/通道关闭/取消）。
pub(super) fn enqueue_memory_frame(
    ctx: &mut MemoryEnqueueCtx,
    queue: &mut EncodeFrameQueue,
    frame_idx: u64,
) -> bool {
    crate::profiling::tracy_span!("app.enqueue_frame");
    let time_sec = frame_idx as f64 / ctx.fps_f64;
    let tempo_changes = &ctx.document.tempo_changes;
    let tick = video_export::seconds_to_tick(time_sec, tempo_changes, ctx.ppq);

    // 键色由渲染线程 GPU 活跃键内核按 tick 推导（1KB 回读）并随帧同序送达，
    // 导出线程零 CPU 扫描（原 `update_playback_key_colors` 的 O(活跃音符) 已移除）。

    // 计算 scroll_x / zoom_x，用于标尺小节号合成
    let video_kb_width = 60.0f32;
    let video_viewport_tick_span = (ctx.ppq * 16).max(1) as f32;
    let video_zoom_x = (ctx.width as f32 - video_kb_width) / video_viewport_tick_span;
    let video_scroll_x = tick as f32 * video_zoom_x;

    // 入队帧合成参数（与帧数据 FIFO 对应）
    queue.push_back(FrameParams {
        scroll_x: video_scroll_x,
        zoom_x: video_zoom_x,
        keyboard_width: video_kb_width,
        ppq: ctx.ppq,
    });

    if ctx.is_cpu_renderer {
        cpu::render_cpu_frame(ctx, tick)
    } else {
        gpu::send_gpu_frame(ctx, tick)
    }
}
