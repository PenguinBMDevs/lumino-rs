//! 合成器初始化与资源编排

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SupportedStreamConfig};
use crossbeam_channel::{bounded, unbounded};
use lumino_memtrace::AllocTag;

use xsynth_core::AudioStreamParams;
use xsynth_core::channel_group::{
    ChannelGroup, ChannelGroupConfig, ParallelismOptions, SynthFormat as XSynthFormat, ThreadCount,
};

use crate::config::{SynthFormat, XSynthRealtimeConfig};
use crate::events::SynthEvent;
use crate::stats::{RealtimeSynthStats, RenderPerfShared};

use super::audio_stream::build_stream;
use super::render::render_thread_loop;
use super::{RealtimeSynth, SendSyncStream};

impl RealtimeSynth {
    /// 获取默认音频输出设备及其配置
    ///
    /// # Panics
    /// 如果没有默认音频设备或无法获取配置，则 panic（无音频设备时应用不可用）。
    fn open_default_device() -> (Device, SupportedStreamConfig) {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("failed to find default audio output device");
        let stream_config = device
            .default_output_config()
            .expect("failed to query default audio output config");
        (device, stream_config)
    }

    /// 使用默认配置和默认音频输出打开合成器
    pub fn open_with_all_defaults() -> Self {
        Self::open_with_default_output(Default::default())
    }

    /// 使用指定配置和默认音频输出打开合成器
    pub fn open_with_default_output(config: XSynthRealtimeConfig) -> Self {
        let (device, stream_config) = Self::open_default_device();
        tracing::info!(
            "RealtimeSynth: 打开音频设备 (device={:?}, sample_rate={}Hz, channels={})",
            device.name().unwrap_or_default(),
            stream_config.sample_rate().0,
            stream_config.channels(),
        );
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

        let (event_sender, event_receiver) = unbounded::<SynthEvent>();
        let (sample_tx, sample_rx) = bounded::<Vec<f32>>(4);
        let (vec_return_tx, vec_return_rx) = unbounded::<Vec<f32>>();
        let vec_return_tx_render = vec_return_tx.clone();

        let render_window = (sample_rate as f64 * config.render_window_ms / 1000.0) as usize;
        let render_len = render_window * channels as usize;

        let (render_thread, running) = lumino_memtrace::with_tag(AllocTag::Audio, || {
            setup_render_thread(
                &config,
                stream_params,
                event_receiver,
                sample_tx.clone(),
                vec_return_rx,
                vec_return_tx_render,
                perf.clone(),
                total_voice_count,
                render_len,
                channels,
                sample_rate,
            )
        });

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
}

/// 创建 ChannelGroup 并启动渲染线程
fn setup_render_thread(
    config: &XSynthRealtimeConfig,
    stream_params: AudioStreamParams,
    event_receiver: crossbeam_channel::Receiver<SynthEvent>,
    sample_tx: crossbeam_channel::Sender<Vec<f32>>,
    vec_return_rx: crossbeam_channel::Receiver<Vec<f32>>,
    vec_return_tx_render: crossbeam_channel::Sender<Vec<f32>>,
    perf_render: Arc<RenderPerfShared>,
    voice_render: Arc<std::sync::atomic::AtomicU64>,
    render_len: usize,
    channels: u16,
    sample_rate: u32,
) -> (thread::JoinHandle<()>, Arc<AtomicBool>) {
    let channel_group = ChannelGroup::new(ChannelGroupConfig {
        channel_init_options: config.channel_init_options.clone(),
        format: match config.format {
            SynthFormat::Midi => XSynthFormat::Midi,
            SynthFormat::Custom { channels } => XSynthFormat::Custom { channels },
        },
        audio_params: stream_params,
        parallelism: ParallelismOptions {
            channel: config.multithreading,
            key: ThreadCount::None,
        },
    });

    let running = Arc::new(AtomicBool::new(true));

    let render_thread = thread::Builder::new()
        .name("lumino-render".into())
        .spawn({
            let running_render = running.clone();
            let perf_render = perf_render.clone();
            let voice_render = voice_render.clone();
            move || {
                render_thread_loop(
                    channel_group,
                    event_receiver,
                    sample_tx,
                    vec_return_rx,
                    vec_return_tx_render,
                    perf_render,
                    voice_render,
                    running_render,
                    render_len,
                    channels,
                    sample_rate,
                )
            }
        })
        .expect("failed to spawn render thread");

    (render_thread, running)
}
