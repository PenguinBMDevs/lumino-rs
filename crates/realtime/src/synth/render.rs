//! 渲染线程循环：时间预算计算、窗口渲染、性能统计

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use lumino_memtrace::AllocTag;

use xsynth_core::channel_group::ChannelGroup;

use crate::events::SynthEvent;
use crate::stats::RenderPerfShared;

use super::event::{drain_events_budgeted, handle_idle, should_skip_render};

/// 渲染线程主循环（在 spawned 线程体内运行）
pub(super) fn render_thread_loop(
    mut channel_group: ChannelGroup,
    event_receiver: Receiver<SynthEvent>,
    sample_tx: Sender<Vec<f32>>,
    vec_return_rx: Receiver<Vec<f32>>,
    vec_return_tx_render: Sender<Vec<f32>>,
    perf_render: Arc<RenderPerfShared>,
    voice_render: Arc<AtomicU64>,
    running_render: Arc<AtomicBool>,
    render_len: usize,
    channels: u16,
    sample_rate: u32,
) {
    // with_tag 必须在 spawned 线程体内设置（外层不会跨线程传播）
    lumino_memtrace::with_tag(AllocTag::Audio, || {
        let (render_timeout_ns, render_window_ms, event_budget_ns) =
            compute_render_budget(render_len, channels, sample_rate);

        while running_render.load(Ordering::Relaxed) {
            if render_iteration(
                &mut channel_group, &event_receiver, &sample_tx,
                &vec_return_rx, &vec_return_tx_render,
                &perf_render, &voice_render,
                render_len, channels, sample_rate,
                render_timeout_ns, render_window_ms, event_budget_ns,
            ) {
                break;
            }
        }
    });
}

/// 执行一次渲染帧迭代。返回 `true` 表示渲染线程应退出循环（音频回调已断开）。
fn render_iteration(
    channel_group: &mut ChannelGroup,
    event_receiver: &Receiver<SynthEvent>,
    sample_tx: &Sender<Vec<f32>>,
    vec_return_rx: &Receiver<Vec<f32>>,
    vec_return_tx_render: &Sender<Vec<f32>>,
    perf_render: &RenderPerfShared,
    voice_render: &Arc<AtomicU64>,
    render_len: usize,
    channels: u16,
    sample_rate: u32,
    render_timeout_ns: u64,
    render_window_ms: u64,
    event_budget_ns: u64,
) -> bool {
    let start = Instant::now();
    let event_deadline = start + Duration::from_nanos(event_budget_ns);
    let event_count = drain_events_budgeted(channel_group, event_receiver, event_deadline);

    if should_skip_render(
        perf_render,
        perf_render.last_render_ns.load(Ordering::Relaxed),
        render_timeout_ns,
        event_count,
    ) {
        return false;
    }

    if handle_idle(
        channel_group, perf_render, event_receiver, sample_tx,
        render_window_ms, event_count,
    ) {
        return false;
    }

    render_window_and_report(
        channel_group, sample_tx, vec_return_rx, vec_return_tx_render,
        perf_render, start, render_len, channels, sample_rate,
        event_count, voice_render,
    )
}

/// 计算渲染线程的三个时间预算：超时阈值、空闲等待窗口、每帧事件处理预算。
fn compute_render_budget(render_len: usize, channels: u16, sample_rate: u32) -> (u64, u64, u64) {
    // 渲染超时阈值：超过 2 倍窗口时间则跳过渲染帧
    let render_timeout_ns =
        (render_len as u64 * 2_000_000_000) / (channels as u64 * sample_rate as u64);

    // 渲染窗口时间（毫秒），用于闲置时睡眠等待
    let render_window_ms = render_timeout_ns / 2_000_000;

    // 每帧事件处理的时间预算：半个渲染窗口。
    // 超过此时间则停止消费事件，剩余事件留到下一帧。
    let event_budget_ns = render_timeout_ns / 4;

    (render_timeout_ns, render_window_ms, event_budget_ns)
}

/// 渲染一个音频窗口，更新 voice 计数与性能统计，并阻塞发送给音频回调。
///
/// 返回 `true` 表示音频回调已断开（发送失败），调用方应退出渲染循环。
#[allow(clippy::too_many_arguments)]
fn render_window_and_report(
    channel_group: &mut ChannelGroup,
    sample_tx: &Sender<Vec<f32>>,
    vec_return_rx: &Receiver<Vec<f32>>,
    vec_return_tx_render: &Sender<Vec<f32>>,
    perf_render: &RenderPerfShared,
    start: Instant,
    render_len: usize,
    channels: u16,
    sample_rate: u32,
    event_count: u64,
    voice_render: &Arc<AtomicU64>,
) -> bool {
    // 获取或重用 Vec（回收池缓冲可能容量不足，若不够则重新分配）
    let mut buf = vec_return_rx
        .try_recv()
        .unwrap_or_else(|_| Vec::with_capacity(render_len));
    if buf.capacity() < render_len {
        buf = Vec::with_capacity(render_len);
    }
    // SAFETY: read_samples_unchecked 保证全覆盖
    unsafe { buf.set_len(render_len); }

    // 渲染一个窗口
    channel_group.read_samples_unchecked(&mut buf);
    voice_render.store(channel_group.voice_count(), Ordering::Relaxed);

    // 阻塞发送给音频回调 — 通道满时渲染线程等待，永不丢弃帧
    if let Err(err) = sample_tx.send(buf) {
        let _ = vec_return_tx_render.send(err.into_inner());
        return true;
    }

    update_render_perf_stats(perf_render, start, render_len, channels, sample_rate, event_count);
    false
}

/// 更新渲染性能统计：渲染耗时、峰值、事件计数和平均负载 EMA。
fn update_render_perf_stats(
    perf_render: &RenderPerfShared,
    start: Instant,
    render_len: usize,
    channels: u16,
    sample_rate: u32,
    event_count: u64,
) {
    let elapsed_ns = start.elapsed().as_nanos() as u64;
    perf_render
        .last_render_ns
        .store(elapsed_ns, Ordering::Relaxed);

    let prev_peak = perf_render.peak_render_ns.load(Ordering::Relaxed);
    if elapsed_ns > prev_peak {
        perf_render
            .peak_render_ns
            .store(elapsed_ns, Ordering::Relaxed);
    }

    perf_render
        .last_event_count
        .store(event_count, Ordering::Relaxed);

    // 渲染负载 EMA
    let expected_ns = (render_len as u64 * 1_000_000_000) / (channels as u64 * sample_rate as u64);
    if expected_ns > 0 {
        let load = (elapsed_ns as f64 / expected_ns as f64).clamp(0.0, 10.0);
        let prev = f64::from_bits(perf_render.average_load.load(Ordering::Relaxed));
        let ema = prev * 0.9 + load * 0.1;
        perf_render
            .average_load
            .store(ema.to_bits(), Ordering::Relaxed);
    }
}
