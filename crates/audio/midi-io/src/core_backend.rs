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
const TARGET_BUFFER_FRAMES: usize = 4096; // 正常水位

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
    pub fn new(soundfont_path: PathBuf, sample_rate: Option<u32>) -> Result<Self, Error> {
        if !soundfont_path.exists() {
            return Err(Error::InitFailed(format!(
                "Soundfont not found: {:?}",
                soundfont_path
            )));
        }

        // ── 协商采样率与声道 ──
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| Error::InitFailed("未找到默认音频输出设备".into()))?;
        let supported = device
            .default_output_config()
            .map_err(|e| Error::InitFailed(format!("获取默认输出配置失败: {e}")))?;
        let device_sr = supported.sample_rate().0;
        let sr = sample_rate.unwrap_or(device_sr);
        // 若请求与设备不一致，优先设备（避免重采样跑调，复刻 yinhe/yinhe 协商）
        let sr = if sr != device_sr {
            tracing::warn!(
                "Core: 请求 sr {sr} 与设备 {device_sr} 不一致，使用设备 sr"
            );
            device_sr
        } else {
            sr
        };
        let channels = supported.channels() as usize;
        // 仅支持 1/2 声道，其余回退立体声
        let channel_count = if channels == 1 {
            ChannelCount::Mono
        } else {
            ChannelCount::Stereo
        };

        let params = AudioStreamParams::new(sr, channel_count);
        let sf: Arc<dyn SoundfontBase> = crate::soundfont_cache::load_soundfont_cached(
            &soundfont_path,
            params,
        )
        .map_err(Error::InitFailed)?;

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
        group.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
            ChannelConfigEvent::SetSoundfonts(vec![sf]),
        )));

        // ── 环形缓冲 ──
        let ring = AudioRing::new(RING_CAPACITY);
        let (mut producer, mut consumer) = ring.split();

        let levels = Arc::new(LevelStore::new());
        let levels_consumer = Arc::clone(&levels);

        // ── cpal 输出流（唯一消费者，零锁） ──
        let stream_config: cpal::StreamConfig = supported.into();
        // cpal 回调：仅 pop_into + 填零 + 峰值统计
        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let n = consumer.pop_into(data);
                    if n < data.len() {
                        data[n..].fill(0.0);
                    }
                    // 峰值统计（master）
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
                    // 通道峰值近似：均写入 master，便于混音台有反馈
                    for ch in levels_consumer.channel_levels.iter() {
                        ch.store(peak.to_bits(), Ordering::Relaxed);
                    }
                },
                |err| tracing::error!("Core cpal stream error: {err}"),
                None,
            )
            .map_err(|e| Error::InitFailed(format!("创建 cpal 流失败: {e}")))?;
        stream
            .play()
            .map_err(|e| Error::InitFailed(format!("启动 cpal 流失败: {e}")))?;

        // ── 渲染线程（唯一生产者） ──
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
                );
            })
            .map_err(|e| Error::InitFailed(format!("启动渲染线程失败: {e}")))?;

        Ok(Self {
            event_tx,
            levels,
            running,
            _stream: Some(stream),
            render_handle: Some(render_handle),
        })
    }

    fn send_event(&self, ev: SynthEvent) {
        let _ = self.event_tx.send(ev);
    }
}

fn render_loop(
    group: &mut ChannelGroup,
    producer: &mut crate::audio_ring::AudioRingProducer,
    event_rx: Receiver<SynthEvent>,
    running: Arc<AtomicBool>,
) {
    // 预分配 scratch，避免热路径堆分配（与 yinhe channel_set 复用一致）
    let channels = group.stream_params().channels.count() as usize;
    let chunk_samples = RENDER_CHUNK_FRAMES * channels;
    let mut scratch = vec![0.0f32; chunk_samples];
    let target_samples = TARGET_BUFFER_FRAMES * channels;

    while running.load(Ordering::Relaxed) {
        // 1. 消费控制事件（非阻塞，批量）
        while let Ok(ev) = event_rx.try_recv() {
            group.send_event(ev);
        }

        // 2. 水位门控：已缓冲 ≥ target 则休眠 1ms
        if producer.len() >= target_samples {
            thread::sleep(Duration::from_millis(1));
            continue;
        }
        if producer.free_space() < chunk_samples {
            thread::sleep(Duration::from_millis(1));
            continue;
        }

        // 3. 渲染一块
        scratch.fill(0.0);
        group.read_samples_unchecked(&mut scratch);
        // 简单限幅（与 export 的 apply_limiter 一致阈值 0.95）
        for s in scratch.iter_mut() {
            if s.abs() > 0.95 {
                *s = s.signum() * 0.95;
            }
        }
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
        // 增益由混音台在渲染后应用；占位保持接口兼容，3/4 可接入 ChannelMix
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
        // 丢弃 stream 与 handle，触发 Drop/join
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
