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
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender, unbounded};
use xsynth_core::{
    AudioPipe, AudioStreamParams, ChannelCount,
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    channel_group::{ChannelGroup, ChannelGroupConfig, ParallelismOptions, SynthEvent, ThreadCount},
    channel::ChannelInitOptions,
    channel_group::SynthFormat,
    soundfont::SoundfontBase,
};

use crate::audio_ring::AudioRing;
use crate::{Error, OutputConnection};

const RING_CAPACITY: usize = 32768; // 16384 立体声帧 ≈340ms@48k，与 yinhe 一致
const RENDER_CHUNK_FRAMES: usize = 512;
const TARGET_BUFFER_FRAMES: usize = 4096; // 正常水位，帧数而非毫秒（yinhe 用帧，避免浮点/采样率耦合）

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
        // ── 采样率/声道协商（优先设备，避免跑调） ──
        let (sr, channel_count, stream_config_opt, device_opt) = {
            let host = cpal::default_host();
            if let Some(device) = host.default_output_device() {
                match device.default_output_config() {
                    Ok(supported) => {
                        let device_sr = supported.sample_rate().0;
                        let req_sr = sample_rate.unwrap_or(device_sr);
                        let sr = if req_sr != device_sr {
                            tracing::warn!(
                                "Core: 请求 sr {req_sr} 与设备 {device_sr} 不一致，使用设备 sr"
                            );
                            device_sr
                        } else {
                            req_sr
                        };
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
                    tracing::warn!("Core: 音色库加载失败 {:?}: {e}，创建空合成器", soundfont_path);
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
            parallelism: ParallelismOptions {
                channel: ThreadCount::Auto,
                key: ThreadCount::None,
            },
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
        let _stream: Option<cpal::Stream> = if let (Some(device), Some(cfg)) =
            (device_opt, stream_config_opt)
        {
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
            thread::sleep(Duration::from_millis(1));
            continue;
        }
        if producer.free_space() < chunk_samples {
            thread::sleep(Duration::from_millis(1));
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
                    ChannelEvent::Audio(ChannelAudioEvent::Control(
                        ControlEvent::PitchBendValue(v),
                    )),
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
