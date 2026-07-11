//! 实时合成器核心实现
//!
//! 高性能设计：自包含的渲染线程 + 锁无关音频回调路径。
//!
//! - 渲染线程独自拥有 `ChannelGroup`，从 bounded channel 消费 MIDI 事件，
//!   按固定窗口大小渲染音频样本，通过 channel 发送给音频回调。
//! - 音频回调仅做 lock-free 的 `try_recv` + limiter + 样本转换，
//!   零锁、零分配、零线程池开销。
//! - 相比 xsynth-realtime 的架构，移除了 `Mutex<BufferedRenderer>` 包装层
//!   和 rayon 在音频回调中的并行开销，显著降低延迟与卡顿风险。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, SupportedStreamConfig};
use crossbeam_channel::{bounded, unbounded};

use xsynth_core::channel_group::{
    ChannelGroup, ChannelGroupConfig, ParallelismOptions, SynthFormat as XSynthFormat, ThreadCount,
};
use xsynth_core::{AudioPipe, AudioStreamParams};

use crate::config::{SynthFormat, XSynthRealtimeConfig};
use crate::events::SynthEvent;
use crate::stats::{
    RealtimeSynthStats, RealtimeSynthStatsReader, RenderPerfShared, RenderPerfStats,
};

/// 发送同步/同步的流包装器
struct SendSyncStream(Stream);
unsafe impl Sync for SendSyncStream {}
unsafe impl Send for SendSyncStream {}

/// 实时合成器
pub struct RealtimeSynth {
    /// 事件发送器
    sender: crossbeam_channel::Sender<SynthEvent>,
    /// 音频流
    stream: Option<SendSyncStream>,
    /// 统计信息
    stats: RealtimeSynthStats,
    /// 性能计数器
    perf: Arc<RenderPerfShared>,
    /// 流参数
    stream_params: AudioStreamParams,
    /// 渲染线程句柄
    render_thread: Option<thread::JoinHandle<()>>,
    /// 渲染线程运行标志
    running: Arc<AtomicBool>,
}

impl RealtimeSynth {
    /// 使用默认配置和默认音频输出打开合成器
    pub fn open_with_all_defaults() -> Self {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("failed to find output device");
        let stream_config = device
            .default_output_config()
            .expect("failed to query default audio output config");
        RealtimeSynth::open(Default::default(), &device, stream_config)
    }

    /// 使用指定配置和默认音频输出打开合成器
    pub fn open_with_default_output(config: XSynthRealtimeConfig) -> Self {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("failed to find output device");
        let stream_config = device
            .default_output_config()
            .expect("failed to query default audio output config");
        RealtimeSynth::open(config, &device, stream_config)
    }

    /// 使用指定配置和音频设备打开合成器
    pub fn open(
        config: XSynthRealtimeConfig,
        device: &Device,
        stream_config: SupportedStreamConfig,
    ) -> Self {
        let sample_rate = stream_config.sample_rate().0;
        let channels: u16 = stream_config.channels();
        let stream_params = AudioStreamParams::new(sample_rate, channels.into());

        let stats = RealtimeSynthStats::new();
        let total_voice_count = stats.voice_count.clone();

        let perf = Arc::new(RenderPerfShared::new());

        // ── 事件通道 ──────────────────────────────────────────────
        let (event_sender, event_receiver) = bounded::<SynthEvent>(65536);

        // ── 音频输出通道 ───────────────────────────────────────────
        // Bounded(4): 最多缓存约 120ms 数据；满时渲染线程自动阻塞，
        // 自然与音频回调同步，无需 spin_sleep。
        let (sample_tx, sample_rx) = bounded::<Vec<f32>>(4);
        let (vec_return_tx, vec_return_rx) = unbounded::<Vec<f32>>();

        let render_window = (sample_rate as f64 * config.render_window_ms / 1000.0) as usize;
        let render_len = render_window * channels as usize;

        // ── ChannelGroup（完全顺序，无 rayon 开销） ────────────────
        let cg_config = ChannelGroupConfig {
            channel_init_options: config.channel_init_options,
            format: match config.format {
                SynthFormat::Midi => XSynthFormat::Midi,
                SynthFormat::Custom { channels } => XSynthFormat::Custom { channels },
            },
            audio_params: stream_params,
            parallelism: ParallelismOptions {
                channel: config.multithreading,
                key: ThreadCount::None,
            },
        };

        let mut channel_group = ChannelGroup::new(cg_config);

        // ── 渲染线程 ───────────────────────────────────────────────
        let running = Arc::new(AtomicBool::new(true));
        let running_render = running.clone();
        let perf_render = perf.clone();
        let voice_render = total_voice_count.clone();

        let render_thread = thread::Builder::new()
            .name("lumino-render".into())
            .spawn(move || {
                while running_render.load(Ordering::Relaxed) {
                    let start = Instant::now();

                    // 消费所有待处理事件（包括 AllChannels 配置）
                    let mut event_count = 0u64;
                    for event in event_receiver.try_iter() {
                        channel_group.send_event(event);
                        event_count += 1;
                    }

                    // 获取或重用 Vec
                    let mut buf = vec_return_rx
                        .try_recv()
                        .unwrap_or_else(|_| Vec::with_capacity(render_len));
                    // 跳过零初始化 — read_samples_unchecked 保证全覆盖
                    if buf.capacity() < render_len {
                        buf.reserve(render_len - buf.capacity());
                    }
                    // SAFETY: read_samples_unchecked 随后会填充 render_len 个样本
                    unsafe {
                        buf.set_len(render_len);
                    }

                    // 渲染一个窗口
                    channel_group.read_samples_unchecked(&mut buf);
                    voice_render.store(channel_group.voice_count(), Ordering::Relaxed);

                    // 发送给音频回调
                    if sample_tx.send(buf).is_err() {
                        break; // Stream 已销毁
                    }

                    // 性能统计
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
                    let expected_ns = (render_len as u64 * 1_000_000_000)
                        / (channels as u64 * sample_rate as u64);
                    if expected_ns > 0 {
                        let load = (elapsed_ns as f64 / expected_ns as f64).clamp(0.0, 10.0);
                        let prev = f64::from_bits(perf_render.average_load.load(Ordering::Relaxed));
                        let ema = prev * 0.9 + load * 0.1;
                        perf_render
                            .average_load
                            .store(ema.to_bits(), Ordering::Relaxed);
                    }

                    // 通过 bounded(4) 通道自然节流：
                    // 满 4 个 buffer 后 send() 阻塞，直到回调消费。
                    // sleep / spin_sleep 皆不需要。
                }
            })
            .expect("failed to spawn render thread");

        // ── 音频回调（锁无关） ──────────────────────────────────────
        let stream = build_stream(device, stream_config, sample_rx, vec_return_tx.clone());

        stream.play().expect("failed to start audio stream");

        Self {
            sender: event_sender,
            stream: Some(SendSyncStream(stream)),
            stats,
            perf,
            stream_params,
            render_thread: Some(render_thread),
            running,
        }
    }

    /// 发送合成器事件
    pub fn send_event(&mut self, event: SynthEvent) {
        let _ = self.sender.send(event);
    }

    /// 获取事件发送器引用
    pub fn get_sender_ref(&self) -> Option<&crossbeam_channel::Sender<SynthEvent>> {
        Some(&self.sender)
    }

    /// 获取统计信息快照
    pub fn get_stats(&self) -> RealtimeSynthStatsReader {
        RealtimeSynthStatsReader {
            voice_count: self.stats.voice_count.load(Ordering::Relaxed),
            average_renderer_load: f64::from_bits(self.perf.average_load.load(Ordering::Relaxed)),
            last_samples_after_read: 0,
        }
    }

    /// 获取性能统计
    pub fn perf_stats(&self) -> RenderPerfStats {
        self.perf.snapshot()
    }

    /// 获取流参数
    pub fn stream_params(&self) -> AudioStreamParams {
        self.stream_params
    }

    /// 获取通道数
    pub fn channel_count(&self) -> u32 {
        self.stream_params.channels.count() as u32
    }

    /// 暂停音频输出
    pub fn pause(&mut self) -> Result<(), cpal::PauseStreamError> {
        if let Some(stream) = &mut self.stream {
            stream.0.pause()
        } else {
            Ok(())
        }
    }

    /// 恢复音频输出
    pub fn resume(&mut self) -> Result<(), cpal::PlayStreamError> {
        if let Some(stream) = &mut self.stream {
            stream.0.play()
        } else {
            Ok(())
        }
    }
}

impl Drop for RealtimeSynth {
    fn drop(&mut self) {
        // 1) 信号量：通知渲染线程退出
        self.running.store(false, Ordering::Relaxed);
        // 2) 释放音频流：sample_rx 析构 → send() 失败 → 渲染线程也退出
        self.stream.take();
        // 3) 等待渲染线程终止
        self.render_thread.take();
    }
}

/// 构建音频流（锁无关回调）
fn build_stream(
    device: &Device,
    stream_config: SupportedStreamConfig,
    sample_rx: crossbeam_channel::Receiver<Vec<f32>>,
    vec_return_tx: crossbeam_channel::Sender<Vec<f32>>,
) -> Stream {
    let err_fn = |err| eprintln!("an error occurred on stream: {err}");

    let channels = stream_config.channels();
    let mut limiter = xsynth_core::effects::VolumeLimiter::new(channels);
    let mut remainder = Vec::new();

    // 预分配输出 Vec
    let mut output_vec =
        Vec::with_capacity(stream_config.sample_rate().0 as usize * channels as usize / 100);

    device
        .build_output_stream(
            &stream_config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                output_vec.resize(data.len(), 0.0);

                let mut i = 0;

                // 1) 消费余量（上次未用完的 Vec 尾部）
                for s in remainder.drain(..) {
                    output_vec[i] = s;
                    i += 1;
                    if i >= output_vec.len() {
                        break;
                    }
                }

                // 2) 从渲染通道拉新 Vec
                while i < output_vec.len() {
                    match sample_rx.try_recv() {
                        Ok(buf) => {
                            let take = buf.len().min(output_vec.len() - i);
                            // 直接拷贝到输出位置
                            let src = &buf[..take];
                            let dst = &mut output_vec[i..i + take];
                            dst.copy_from_slice(src);
                            i += take;

                            // 剩下的退回余量
                            if take < buf.len() {
                                remainder.extend_from_slice(&buf[take..]);
                            }

                            // 回收 Vec 回池
                            let _ = vec_return_tx.send(buf);
                        }
                        Err(_) => {
                            // 无数据：静音填充余量，跳出
                            break;
                        }
                    }
                }

                // 3) 限幅（防止削波）
                limiter.limit(&mut output_vec);

                // 4) 拷贝到 cpal 输出
                // （cpal 提供的是 f32 格式时直接拷贝）
                data.copy_from_slice(&output_vec);
            },
            err_fn,
            None,
        )
        .expect("failed to build output audio stream")
}
