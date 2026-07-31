//! 事件处理：时间预算内消费、超时检测、闲置检测

use std::sync::atomic::Ordering;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};

use xsynth_core::channel_group::ChannelGroup;

use crate::events::SynthEvent;
use crate::stats::RenderPerfShared;

/// 在事件时间预算内消费待处理事件，返回本帧处理的事件数。
///
/// 一次性排空可能耗时数十秒，导致音频回调得不到数据，故每 1024 个事件
/// 检查一次 `event_deadline`，超时即停止，剩余事件留到下一帧。
pub(super) fn drain_events_budgeted(
    channel_group: &mut ChannelGroup,
    event_receiver: &Receiver<SynthEvent>,
    event_deadline: Instant,
) -> u64 {
    let mut event_count = 0u64;
    for event in event_receiver.try_iter() {
        channel_group.send_event(event);
        event_count += 1;
        // 每 1024 个事件检查一次时间预算
        if event_count & 0x3FF == 0 && Instant::now() > event_deadline {
            break;
        }
    }
    event_count
}

/// 检查上一帧是否超时：若渲染赶不上，重置超时标记并跳过本次渲染。
///
/// 返回 `true` 表示应跳过本帧渲染（仅消费事件）。
pub(super) fn should_skip_render(
    perf_render: &RenderPerfShared,
    prev_render_ns: u64,
    render_timeout_ns: u64,
    event_count: u64,
) -> bool {
    if prev_render_ns > render_timeout_ns {
        // 限制日志频率：每 10 次超时只输出 1 次，避免日志 I/O 拖慢渲染线程
        if event_count > 0 || prev_render_ns > render_timeout_ns * 10 {
            tracing::warn!(
                "lumino-render: 渲染超时 ({}ns > {}ns)，跳过渲染帧，事件数={}",
                prev_render_ns,
                render_timeout_ns,
                event_count,
            );
        }
        // 重置超时标记，下一次迭代重新尝试渲染
        perf_render.last_render_ns.store(0, Ordering::Relaxed);
        // 占位统计：更新事件计数，保持渲染线程存活
        perf_render
            .last_event_count
            .store(event_count, Ordering::Relaxed);
        // 短暂 yield 避免 busy-loop
        std::thread::yield_now();
        true
    } else {
        false
    }
}

/// 闲置检测：没有事件且样本通道已满时，阻塞等待事件到达（或超时）。
///
/// 返回 `true` 表示已处理闲置分支并应 `continue` 到下一帧。
pub(super) fn handle_idle(
    channel_group: &mut ChannelGroup,
    perf_render: &RenderPerfShared,
    event_receiver: &Receiver<SynthEvent>,
    sample_tx: &Sender<Vec<f32>>,
    render_window_ms: u64,
    event_count: u64,
) -> bool {
    if event_count == 0 && sample_tx.len() >= 4 {
        perf_render.last_render_ns.store(0, Ordering::Relaxed);
        perf_render
            .last_event_count
            .store(event_count, Ordering::Relaxed);
        // 等待事件到达或超时；收到的事件立即处理，避免丢失
        if let Ok(event) =
            event_receiver.recv_timeout(std::time::Duration::from_millis(render_window_ms))
        {
            channel_group.send_event(event);
        }
        true
    } else {
        false
    }
}
