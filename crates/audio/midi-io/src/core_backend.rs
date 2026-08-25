//! Core 后端：`xsynth-core ChannelGroup` + `AudioRing SPSC` + `cpal` 零锁回调
//!
//! 复刻 `yinhe` 的三线程模型（`cpal` 回调消费者 + `renderer` 单生产者 + 可选 `worker`），
//! 与 `Realtime（xsynth-realtime）` 相比：`render` 线程独占 `ChannelGroup`，
//! `cpal` 仅做 `Atomic` + `ring.pop_into` 填零，渲染侧预分配 `scratch` 零堆分配。

use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender, unbounded};
use xsynth_core::{
    AudioPipe, AudioStreamParams, ChannelCount,
    channel::ChannelInitOptions,
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    channel_group::SynthFormat,
    channel_group::{ChannelGroup, ChannelGroupConfig, ParallelismOptions, SynthEvent},
    soundfont::SoundfontBase,
};

use crate::audio_ring::AudioRing;
use crate::{Error, OutputConnection};

const RING_CAPACITY: usize = 32768; // 16384 立体声帧 ≈340ms@48k，与 yinhe 一致
const RENDER_CHUNK_FRAMES: usize = 512;
const TARGET_BUFFER_FRAMES: usize = 4096; // 正常水位，帧数而非毫秒（yinhe 用帧，避免浮点/采样率耦合）
/// 混合等待的自旋尾长：粗睡到只剩该时长后自旋，保证唤醒精度（Windows 默认定时器
/// 分辨率 15.6ms，纯 sleep(1ms) 实际睡 15.6ms → ring 产出跟不上消费 → 欠载卡顿）。
const WAIT_SPIN_TAIL: Duration = Duration::from_micros(300);

/// Windows 高精度定时器守卫：存活期间将系统定时器分辨率提升到 1ms。
///
/// 渲染线程的 `thread::sleep` 在 Windows 上默认受 15.6ms 定时中断限制，
/// 必须 `timeBeginPeriod(1)` 才能真正睡约 1ms（与播放线程 `playback/manager.rs`
/// 的睡眠+自旋策略互补：timeBeginPeriod 让 sleep 段更准，自旋尾兜底）。
#[cfg(target_os = "windows")]
struct TimerResolutionGuard;

#[cfg(target_os = "windows")]
impl TimerResolutionGuard {
    fn new() -> Self {
        // 失败仅意味着保持原分辨率，非致命
        unsafe { windows::Win32::Media::timeBeginPeriod(1) };
        Self
    }
}

#[cfg(target_os = "windows")]
impl Drop for TimerResolutionGuard {
    fn drop(&mut self) {
        unsafe { windows::Win32::Media::timeEndPeriod(1) };
    }
}

/// 睡眠+自旋混合等待：大部分时间粗睡，最后 `WAIT_SPIN_TAIL` 自旋到精确唤醒。
fn precise_wait(wait: Duration) {
    let target = Instant::now() + wait;
    let coarse = wait.saturating_sub(WAIT_SPIN_TAIL);
    if coarse > Duration::ZERO {
        thread::sleep(coarse);
    }
    while Instant::now() < target {
        std::hint::spin_loop();
    }
}

/// 协商采样率：请求值不在设备任何 f32 输出配置的支持范围内时，回退到设备默认采样率。
/// （移植自 yinhe `spawn.rs::negotiate_sample_rate`）
fn negotiate_sample_rate(device: &cpal::Device, requested: u32, device_default: u32) -> u32 {
    let supported = match device.supported_output_configs() {
        Ok(configs) => configs
            .filter(|c| c.sample_format() == cpal::SampleFormat::F32)
            .any(|c| requested >= c.min_sample_rate().0 && requested <= c.max_sample_rate().0),
        Err(_) => return device_default,
    };
    if supported {
        requested
    } else {
        tracing::warn!(
            "Core: 请求采样率 {requested}Hz 不被设备支持，回退设备默认 {device_default}Hz"
        );
        device_default
    }
}

#[derive(Debug)]
struct LevelStore {
    channel_levels: [AtomicU32; 16],
    master: AtomicU32,
}

impl LevelStore {
    fn new() -> Self {
        Self {
            channel_levels: std::array::from_fn(|_| AtomicU32::new(0)),
            master: AtomicU32::new(0),
        }
    }
}

/// Core 后端输出连接
pub struct CoreOutput {
    event_tx: Sender<SynthEvent>,
    levels: Arc<LevelStore>,
    running: Arc<AtomicBool>,
    _stream: Option<cpal::Stream>,
    render_handle: Option<JoinHandle<()>>,
    /// Windows 高分辨率定时器守卫（其他平台为空），随连接关闭恢复系统默认分辨率
    #[cfg(target_os = "windows")]
    _timer_guard: TimerResolutionGuard,
}

// cpal::Stream 在 macOS 上为 !Send（CoreAudio 内部 Mutex 非 Send），但我们在 UI 线程创建
// 并在同一线程 Drop，跨线程仅通过 Atomic/Channel 通信，标记 CoreOutput 为 Send 以满足
// OutputConnection: Send 契约
unsafe impl Send for CoreOutput {}

impl CoreOutput {
    pub fn new(
        soundfont_path: PathBuf,
        sample_rate: Option<u32>,
        buffer_frames: Option<u32>,
    ) -> Result<Self, Error> {
        // 尽早提升 Windows 定时器分辨率，保证渲染线程 sleep 精度
        #[cfg(target_os = "windows")]
        let timer_guard = TimerResolutionGuard::new();

        // ── 采样率/声道协商（请求值在设备支持范围内则生效，否则回退设备默认） ──
        let (sr, channel_count, stream_config_opt, device_opt) = {
            let host = cpal::default_host();
            if let Some(device) = host.default_output_device() {
                match device.default_output_config() {
                    Ok(supported) => {
                        let device_sr = supported.sample_rate().0;
                        let sr = negotiate_sample_rate(
                            &device,
                            sample_rate.unwrap_or(device_sr),
                            device_sr,
                        );
                        let channels = supported.channels() as usize;
                        let cc = if channels == 1 {
                            ChannelCount::Mono
                        } else {
                            ChannelCount::Stereo
                        };
                        let cfg: cpal::StreamConfig = supported.into();
                        (sr, cc, Some(cfg), Some(device))
                    }
                    Err(e) => {
                        tracing::warn!("Core: 获取默认输出配置失败: {e}，回退 44100 Stereo");
                        (
                            sample_rate.unwrap_or(44100),
                            ChannelCount::Stereo,
                            None,
                            None,
                        )
                    }
                }
            } else {
                tracing::warn!("Core: 未找到默认输出设备，回退 44100 Stereo（无声输出，仅 ring）");
                (
                    sample_rate.unwrap_or(44100),
                    ChannelCount::Stereo,
                    None,
                    None,
                )
            }
        };

        let params = AudioStreamParams::new(sr, channel_count);

        // ── SoundFont：缺失时仍创建空 Group，保证 ring/stream 可用（静音），后续重载 ──
        let sf_opt: Option<Arc<dyn SoundfontBase>> = if soundfont_path.as_os_str().is_empty() {
            tracing::warn!("Core: 音色库路径为空，创建空合成器（静音，直到设置音色库）");
            None
        } else if !soundfont_path.exists() {
            tracing::warn!("Core: 音色库不存在 {:?}，创建空合成器", soundfont_path);
            None
        } else {
            match crate::soundfont_cache::load_soundfont_cached(&soundfont_path, params) {
                Ok(sf) => Some(sf),
                Err(e) => {
                    tracing::warn!(
                        "Core: 音色库加载失败 {:?}: {e}，创建空合成器",
                        soundfont_path
                    );
                    None
                }
            }
        };

        let group_config = ChannelGroupConfig {
            channel_init_options: ChannelInitOptions {
                fade_out_killing: true,
            },
            format: SynthFormat::Midi,
            audio_params: params,
            // 键级并行（AUTO_PER_KEY）与 Realtime（threads=Auto → 每 VoiceChannel
            // 共享 rayon 池渲染 key）对齐：Black MIDI 密集段大量 voice 集中在
            // 少数通道，仅通道级并行无法吃满多核。渲染块 512 帧 ≈10.6ms@48k，
            // 远大于 xsynth 文档提示的 sub-1ms 开销阈值，键级并行的调度成本可忽略。
            parallelism: ParallelismOptions::AUTO_PER_KEY,
        };
        let mut group = ChannelGroup::new(group_config);
        if let Some(sf) = sf_opt {
            group.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
                ChannelConfigEvent::SetSoundfonts(vec![sf]),
            )));
        }

        // ── 环形缓冲 ──
        let ring = AudioRing::new(RING_CAPACITY);
        let (mut producer, mut consumer) = ring.split();

        let levels = Arc::new(LevelStore::new());
        let levels_consumer = Arc::clone(&levels);

        // ── cpal 输出流（唯一消费者，零锁）──
        let _stream: Option<cpal::Stream> =
            if let (Some(device), Some(cfg)) = (device_opt, stream_config_opt) {
                match device.build_output_stream(
                    &cfg,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let n = consumer.pop_into(data);
                        if n < data.len() {
                            data[n..].fill(0.0);
                        }
                        let mut peak: f32 = 0.0;
                        for &s in data.iter() {
                            let a = s.abs();
                            if a > peak {
                                peak = a;
                            }
                        }
                        levels_consumer
                            .master
                            .store(peak.to_bits(), Ordering::Relaxed);
                        for ch in levels_consumer.channel_levels.iter() {
                            ch.store(peak.to_bits(), Ordering::Relaxed);
                        }
                    },
                    |err| tracing::error!("Core cpal stream error: {err}"),
                    None,
                ) {
                    Ok(s) => {
                        if let Err(e) = s.play() {
                            tracing::error!("Core: 启动 cpal 流失败: {e}");
                            None
                        } else {
                            tracing::info!("Core: cpal 流已启动 sr={} ch={:?}", sr, channel_count);
                            Some(s)
                        }
                    }
                    Err(e) => {
                        tracing::error!("Core: 创建 cpal 流失败: {e}");
                        None
                    }
                }
            } else {
                tracing::warn!("Core: 无可用 cpal 流，进入离线 ring 模式（无 audible 输出）");
                None
            };

        // ── 渲染线程（唯一生产者） ──
        let target_frames = buffer_frames.unwrap_or(TARGET_BUFFER_FRAMES as u32) as usize;
        let (event_tx, event_rx): (Sender<SynthEvent>, Receiver<SynthEvent>) = unbounded();
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let render_handle = thread::Builder::new()
            .name("lumino-core-renderer".into())
            .spawn(move || {
                render_loop(
                    &mut group,
                    &mut producer,
                    event_rx,
                    running_clone,
                    target_frames,
                );
            })
            .map_err(|e| Error::InitFailed(format!("启动渲染线程失败: {e}")))?;

        Ok(Self {
            event_tx,
            levels,
            running,
            _stream,
            render_handle: Some(render_handle),
            #[cfg(target_os = "windows")]
            _timer_guard: timer_guard,
        })
    }

    fn send_event(&self, ev: SynthEvent) {
        let _ = self.event_tx.send(ev);
    }
}

struct SingleChannelLimiter {
    loudness: f32,
    attack: f32,
    falloff: f32,
    strength: f32,
    min_thresh: f32,
}
impl SingleChannelLimiter {
    fn new() -> Self {
        Self {
            loudness: 1.0,
            attack: 100.0,
            falloff: 16000.0,
            strength: 1.0,
            min_thresh: 1.0,
        }
    }
    fn limit(&mut self, val: f32) -> f32 {
        let abs = val.abs();
        if self.loudness > abs {
            self.loudness = (self.loudness * self.falloff + abs) / (self.falloff + 1.0);
        } else {
            self.loudness = (self.loudness * self.attack + abs) / (self.attack + 1.0);
        }
        if self.loudness < self.min_thresh {
            self.loudness = self.min_thresh;
        }
        val / (self.loudness * self.strength + 2.0 * (1.0 - self.strength)) / 2.0
    }
}
struct VolumeLimiter {
    channels: Vec<SingleChannelLimiter>,
    ch: usize,
}
impl VolumeLimiter {
    fn new(ch: u16) -> Self {
        Self {
            channels: (0..ch).map(|_| SingleChannelLimiter::new()).collect(),
            ch: ch as usize,
        }
    }
    fn limit(&mut self, buf: &mut [f32]) {
        for (i, s) in buf.iter_mut().enumerate() {
            *s = self.channels[i % self.ch].limit(*s);
        }
    }
}

fn render_loop(
    group: &mut ChannelGroup,
    producer: &mut crate::audio_ring::AudioRingProducer,
    event_rx: Receiver<SynthEvent>,
    running: Arc<AtomicBool>,
    target_frames: usize,
) {
    let channels = group.stream_params().channels.count() as usize;
    let chunk_samples = RENDER_CHUNK_FRAMES * channels;
    let mut scratch = vec![0.0f32; chunk_samples];
    let target_samples = target_frames * channels;
    let mut limiter = VolumeLimiter::new(channels as u16);

    while running.load(Ordering::Relaxed) {
        while let Ok(ev) = event_rx.try_recv() {
            group.send_event(ev);
        }
        if producer.len() >= target_samples {
            precise_wait(Duration::from_millis(1));
            continue;
        }
        if producer.free_space() < chunk_samples {
            precise_wait(Duration::from_millis(1));
            continue;
        }
        scratch.fill(0.0);
        group.read_samples_unchecked(&mut scratch);
        limiter.limit(&mut scratch);
        let _ = producer.push_slice(&scratch);
    }
}

impl OutputConnection for CoreOutput {
    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        let status = data[0] & 0xF0;
        let channel = (data[0] & 0x0F) as u32;
        let b1 = data[1];
        let b2 = data[2];
        let ev = match status {
            0x80 => SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: b1 & 0x7F }),
            ),
            0x90 => {
                let vel = b2 & 0x7F;
                if vel == 0 {
                    SynthEvent::Channel(
                        channel,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: b1 & 0x7F }),
                    )
                } else {
                    SynthEvent::Channel(
                        channel,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                            key: b1 & 0x7F,
                            vel,
                        }),
                    )
                }
            }
            0xB0 => SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(b1, b2))),
            ),
            0xC0 => SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(b1)),
            ),
            0xE0 => {
                let bend = ((b1 as u16) | ((b2 as u16) << 7)) as f32;
                let v = bend / 8192.0 - 1.0;
                SynthEvent::Channel(
                    channel,
                    ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                        v,
                    ))),
                )
            }
            _ => return Ok(()),
        };
        self.send_event(ev);
        Ok(())
    }

    fn all_notes_off(&mut self) -> Result<(), Error> {
        self.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::AllNotesOff,
        )));
        Ok(())
    }

    fn reset_control(&mut self) -> Result<(), Error> {
        self.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::ResetControl,
        )));
        Ok(())
    }

    fn set_channel_gain(&mut self, _ch: u8, _gain: f32) -> Result<(), Error> {
        Ok(())
    }

    fn set_channel_pan(&mut self, _ch: u8, _pan: f32) -> Result<(), Error> {
        Ok(())
    }

    fn get_channel_levels(&self) -> [f32; 16] {
        let mut out = [0.0f32; 16];
        for (i, a) in self.levels.channel_levels.iter().enumerate() {
            out[i] = f32::from_bits(a.load(Ordering::Relaxed));
        }
        out
    }

    fn get_master_level(&self) -> f32 {
        f32::from_bits(self.levels.master.load(Ordering::Relaxed))
    }

    fn close(mut self: Box<Self>) {
        self.running.store(false, Ordering::Relaxed);
        self._stream.take();
        if let Some(h) = self.render_handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for CoreOutput {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
