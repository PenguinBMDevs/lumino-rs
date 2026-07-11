//! 实时事件处理引擎
//!
//! 将 `ChannelGroup` 的事件输入与音频渲染解耦，提供单事件与批量事件两种输入路径。
//! 批量路径通过独立的 batch 通道传输 `Vec<SynthEvent>`，在极端高 NPS 场景下显著
//! 降低 crossbeam 通道的 per-event 原子操作与内存分配开销。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Select, Sender, bounded, unbounded};
use xsynth_core::channel_group::{
    ChannelGroup, ChannelGroupConfig, ParallelismOptions, SynthFormat as XSynthFormat, ThreadCount,
};
use xsynth_core::{AudioPipe, AudioStreamParams};

use crate::config::{SynthFormat, XSynthRealtimeConfig};
use crate::events::SynthEvent;
use crate::stats::RenderPerfShared;

/// 批量事件分块大小。
/// 在降低通道开销的同时，避免单批次过大导致渲染帧时间预算被突破。
const EVENT_BATCH_SIZE: usize = 8192;

/// 事件处理时间预算 = 渲染超时阈值 / EVENT_BUDGET_DIVISOR。
const EVENT_BUDGET_DIVISOR: u64 = 2;

/// 每处理 1024 个事件检查一次时间预算。
const EVENT_CHECK_MASK: u64 = 0x3FF;

/// 批量事件发送错误类型。
pub type SendEventsError = crossbeam_channel::SendError<Vec<SynthEvent>>;

/// 实时事件处理引擎。
///
/// 内部持有一个 `ChannelGroup`，并通过双通道（单事件 + 批量事件）接收外部输入。
/// 批量路径是本结构体相比逐事件发送的核心性能优化点。
pub struct RealtimeEventEngine {
    event_tx: Sender<SynthEvent>,
    event_rx: Receiver<SynthEvent>,
    batch_tx: Sender<Vec<SynthEvent>>,
    batch_rx: Receiver<Vec<SynthEvent>>,
    buffer_return_tx: Sender<Vec<f32>>,
    buffer_return_rx: Receiver<Vec<f32>>,
    channel_group: ChannelGroup,
    stream_params: AudioStreamParams,
    render_window: usize,
    render_len: usize,
    perf: Arc<RenderPerfShared>,
    voice_count: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
    pending_single: Option<SynthEvent>,
    pending_batches: Vec<Vec<SynthEvent>>,
}

impl RealtimeEventEngine {
    /// 使用指定配置创建引擎。
    ///
    /// `voice_count` 为外部共享的活跃 voice 计数器，渲染完成后会自动更新。
    pub fn new(
        config: XSynthRealtimeConfig,
        stream_params: AudioStreamParams,
        voice_count: Arc<AtomicU64>,
    ) -> Self {
        let sample_rate = stream_params.sample_rate;
        let channels = stream_params.channels.count() as usize;

        let render_window = (sample_rate as f64 * config.render_window_ms / 1000.0) as usize;
        let render_len = render_window * channels;

        let (event_tx, event_rx) = unbounded::<SynthEvent>();
        let (batch_tx, batch_rx) = unbounded::<Vec<SynthEvent>>();
        let (buffer_return_tx, buffer_return_rx) = bounded::<Vec<f32>>(4);

        let cg_config = ChannelGroupConfig {
            channel_init_options: config.channel_init_options,
            format: match config.format {
                SynthFormat::Midi => XSynthFormat::Midi,
                SynthFormat::Custom { channels: ch } => XSynthFormat::Custom { channels: ch },
            },
            audio_params: stream_params,
            parallelism: ParallelismOptions {
                channel: config.multithreading,
                key: ThreadCount::None,
            },
        };

        let channel_group = ChannelGroup::new(cg_config);

        Self {
            event_tx,
            event_rx,
            batch_tx,
            batch_rx,
            buffer_return_tx,
            buffer_return_rx,
            channel_group,
            stream_params,
            render_window,
            render_len,
            perf: Arc::new(RenderPerfShared::new()),
            voice_count,
            running: Arc::new(AtomicBool::new(true)),
            pending_single: None,
            pending_batches: Vec::new(),
        }
    }

    /// 获取流参数。
    pub fn stream_params(&self) -> AudioStreamParams {
        self.stream_params
    }

    /// 获取渲染窗口大小（样本数）。
    pub fn render_window(&self) -> usize {
        self.render_window
    }

    /// 获取单事件发送器。
    pub fn event_sender(&self) -> Sender<SynthEvent> {
        self.event_tx.clone()
    }

    /// 获取批量事件发送器。
    pub fn batch_sender(&self) -> Sender<Vec<SynthEvent>> {
        self.batch_tx.clone()
    }

    /// 获取缓冲区回收发送器（供音频回调使用）。
    ///
    /// 当前仅由 `synth.rs` 在集成时使用，保留给未来内部调用。
    #[allow(dead_code)]
    pub(crate) fn buffer_return_sender(&self) -> Sender<Vec<f32>> {
        self.buffer_return_tx.clone()
    }

    /// 获取共享性能计数器。
    ///
    /// 当前仅由 `synth.rs` 在集成时使用，保留给未来内部调用。
    #[allow(dead_code)]
    pub(crate) fn perf_shared(&self) -> Arc<RenderPerfShared> {
        self.perf.clone()
    }

    /// 获取共享 voice 计数器。
    ///
    /// 当前仅由 `synth.rs` 在集成时使用，保留给未来内部调用。
    #[allow(dead_code)]
    pub(crate) fn voice_count_arc(&self) -> Arc<AtomicU64> {
        self.voice_count.clone()
    }

    /// 获取性能统计快照。
    pub fn perf_stats(&self) -> crate::stats::RenderPerfStats {
        self.perf.snapshot()
    }

    /// 获取运行标志。
    ///
    /// 当前仅由 `synth.rs` 在集成时使用，保留给未来内部调用。
    #[allow(dead_code)]
    pub(crate) fn running(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    /// 发送单个事件。
    ///
    /// 失败仅发生在渲染线程已退出时，返回值被忽略以兼容旧接口。
    pub fn send_event(&self, event: SynthEvent) {
        let _ = self.event_tx.send(event);
    }

    /// 批量发送事件。
    ///
    /// 将迭代器拆分为大小为 [`EVENT_BATCH_SIZE`] 的块，通过 batch 通道一次性传输，
    /// 显著降低 crossbeam 的 per-event 开销。
    pub fn send_events<I: IntoIterator<Item = SynthEvent>>(
        &self,
        events: I,
    ) -> Result<(), SendEventsError> {
        let mut batch = Vec::with_capacity(EVENT_BATCH_SIZE);
        for event in events {
            batch.push(event);
            if batch.len() == EVENT_BATCH_SIZE {
                let full = std::mem::replace(&mut batch, Vec::with_capacity(EVENT_BATCH_SIZE));
                self.batch_tx.send(full)?;
            }
        }
        if !batch.is_empty() {
            self.batch_tx.send(batch)?;
        }
        Ok(())
    }

    /// 将渲染后的缓冲区回收至池中，供下一帧复用。
    pub fn return_buffer(&self, buf: Vec<f32>) {
        let _ = self.buffer_return_tx.send(buf);
    }

    /// 当前是否无待处理事件。
    pub fn is_idle(&self) -> bool {
        self.pending_single.is_none()
            && self.pending_batches.is_empty()
            && self.event_rx.is_empty()
            && self.batch_rx.is_empty()
    }

    /// 等待事件到达或超时。
    ///
    /// 返回 `true` 表示收到事件并已放入 pending 队列；`false` 表示超时或通道已断开。
    pub fn wait_for_event(&mut self, timeout: Duration) -> bool {
        if self.pending_single.is_some() || !self.pending_batches.is_empty() {
            return true;
        }

        let mut sel = Select::new();
        let ev_idx = sel.recv(&self.event_rx);
        let ba_idx = sel.recv(&self.batch_rx);

        match sel.select_timeout(timeout) {
            Ok(op) => {
                if op.index() == ev_idx {
                    match op.recv(&self.event_rx) {
                        Ok(ev) => {
                            self.pending_single = Some(ev);
                            true
                        }
                        Err(_) => false,
                    }
                } else if op.index() == ba_idx {
                    match op.recv(&self.batch_rx) {
                        Ok(batch) => {
                            if !batch.is_empty() {
                                self.pending_batches.push(batch);
                            }
                            true
                        }
                        Err(_) => false,
                    }
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    /// 处理待处理事件并渲染一帧音频。
    ///
    /// 返回 `Some(buf)` 表示成功渲染；`None` 表示本帧仅处理事件（渲染被跳过）。
    /// 调用方应通过 [`Self::return_buffer`] 归还缓冲区。
    pub fn render_frame(&mut self) -> Option<Vec<f32>> {
        if !self.running.load(Ordering::Relaxed) {
            return None;
        }

        let start = Instant::now();
        let channels = self.stream_params.channels.count();
        let sample_rate = self.stream_params.sample_rate;

        // 2 倍渲染窗口时间作为超时阈值。
        let render_timeout_ns =
            (self.render_len as u64 * 2_000_000_000) / (channels as u64 * sample_rate as u64);

        let prev_render_ns = self.perf.last_render_ns.load(Ordering::Relaxed);
        let skip_render = prev_render_ns > render_timeout_ns;

        let event_budget_ns = render_timeout_ns / EVENT_BUDGET_DIVISOR;
        let event_deadline = start + Duration::from_nanos(event_budget_ns);
        let mut event_count = 0u64;

        // 1) 处理缓存的单事件。
        if let Some(event) = self.pending_single.take() {
            self.channel_group.send_event(event);
            event_count += 1;
        }

        // 2) 处理缓存的批量事件（整批处理，仅在批次之间检查预算）。
        while let Some(batch) = self.pending_batches.pop() {
            for event in batch {
                self.channel_group.send_event(event);
                event_count += 1;
            }
            if event_count & EVENT_CHECK_MASK == 0 && Instant::now() > event_deadline {
                break;
            }
        }

        // 3) 处理新到达的批量事件。
        if Instant::now() <= event_deadline {
            for batch in self.batch_rx.try_iter() {
                for event in batch {
                    self.channel_group.send_event(event);
                    event_count += 1;
                }
                if event_count & EVENT_CHECK_MASK == 0 && Instant::now() > event_deadline {
                    break;
                }
            }
        }

        // 4) 处理新到达的单事件。
        if Instant::now() <= event_deadline {
            for event in self.event_rx.try_iter() {
                self.channel_group.send_event(event);
                event_count += 1;
                if event_count & EVENT_CHECK_MASK == 0 && Instant::now() > event_deadline {
                    break;
                }
            }
        }

        // 若上一帧已超时，则本帧只消费事件、不渲染，避免音频欠载持续恶化。
        if skip_render {
            self.perf.last_render_ns.store(0, Ordering::Relaxed);
            self.perf
                .last_event_count
                .store(event_count, Ordering::Relaxed);
            return None;
        }

        // 5) 从池中获取或分配输出缓冲区。
        let mut buf = self
            .buffer_return_rx
            .try_recv()
            .unwrap_or_else(|_| Vec::with_capacity(self.render_len));
        if buf.capacity() < self.render_len {
            buf.reserve(self.render_len - buf.capacity());
        }
        // SAFETY: `read_samples_unchecked` 会填充 `render_len` 个样本。
        unsafe {
            buf.set_len(self.render_len);
        }

        // 6) 渲染。
        self.channel_group.read_samples_unchecked(&mut buf);
        let vc = self.channel_group.voice_count();
        self.voice_count.store(vc, Ordering::Relaxed);

        // 7) 更新性能统计。
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        self.perf
            .last_render_ns
            .store(elapsed_ns, Ordering::Relaxed);
        let prev_peak = self.perf.peak_render_ns.load(Ordering::Relaxed);
        if elapsed_ns > prev_peak {
            self.perf
                .peak_render_ns
                .store(elapsed_ns, Ordering::Relaxed);
        }
        self.perf
            .last_event_count
            .store(event_count, Ordering::Relaxed);

        let expected_ns =
            (self.render_len as u64 * 1_000_000_000) / (channels as u64 * sample_rate as u64);
        if expected_ns > 0 {
            let load = (elapsed_ns as f64 / expected_ns as f64).clamp(0.0, 10.0);
            let prev = f64::from_bits(self.perf.average_load.load(Ordering::Relaxed));
            let ema = prev * 0.9 + load * 0.1;
            self.perf
                .average_load
                .store(ema.to_bits(), Ordering::Relaxed);
        }

        Some(buf)
    }
}

#[cfg(test)]
mod tests;
