//! LGS (GPU) 合成后端：基于 `lumino-gpu-synth` 的 GPU 加速合成器。
//!
//! 与 XSynth 同源的设计：后端持有 `AudioPlayback`（内部自带 cpal 音频设备），
//! 只通过共享的事件发送器把 MIDI 事件转发给渲染线程；音频输出完全由 GPU
//! 合成管线产生。所有输出连接共享同一个事件发送器，因此「内置 MIDI 输出组」
//! 中可创建多个连接到同一渲染线程，无需第二个 GPU 实例。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use lumino_gpu_synth::audio::playback::AudioPlayback;
use lumino_gpu_synth::midi::MidiEvent;
use lumino_gpu_synth::{GpuSynth, InterpolationMode, SynthConfig};

use crate::constants::*;
use crate::{Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo};

/// LGS (GPU) 后端初始化选项
#[derive(Debug, Clone)]
pub struct LgsOptions {
    /// 渲染采样率（Hz）
    pub sample_rate: u32,
    /// 每块渲染的音频帧数（GPU 一次 dispatch 的帧数）
    pub block_size: usize,
    /// 每个 (通道, 键) 的最大同音数
    pub max_voices_per_key: usize,
    /// 是否使用 64 点 sinc 高质量插值（否则线性插值）
    pub use_sinc: bool,
    /// 响度(力度)过滤阈值：MIDI 力度 <= 此值的音符不发声（0=关闭过滤）
    pub velocity_filter_threshold: u8,
}

/// LGS (GPU) 软件合成后端
pub struct Lgs {
    /// 持有 AudioPlayback 以保持 GPU 渲染线程与 cpal 音频流存活
    /// （包在 Mutex 中仅为满足 `Api: Send + Sync`；正常运行无需加锁访问）
    _playback: Arc<Mutex<AudioPlayback>>,
    /// 共享事件发送器（所有输出连接通过它向渲染线程转发 MIDI 事件）
    event_tx: Arc<Mutex<Option<mpsc::Sender<(u8, MidiEvent)>>>>,
    /// 响度(力度)过滤阈值（所有输出连接共享，note_on 时实时丢弃过轻音符）
    velocity_filter: Arc<AtomicU8>,
    /// 音色库路径（重建/重初始化时重用）
    #[allow(dead_code)]
    soundfont_path: PathBuf,
    #[allow(dead_code)]
    options: LgsOptions,
    version: String,
}

impl Lgs {
    /// 使用指定音色库路径与选项创建 LGS 后端
    pub fn new(soundfont_path: &Path, options: &LgsOptions) -> Result<Self, Error> {
        tracing::info!("LGS (GPU): 初始化，音色库路径: {:?}", soundfont_path);

        if !soundfont_path.exists() {
            return Err(Error::InitFailed(format!(
                "Soundfont file not found: {:?}",
                soundfont_path
            )));
        }

        let config = SynthConfig {
            sample_rate: options.sample_rate,
            block_size: options.block_size,
            max_voices_per_key: options.max_voices_per_key,
            interpolation: if options.use_sinc {
                InterpolationMode::Point64Sinc
            } else {
                InterpolationMode::Linear
            },
            ..SynthConfig::default()
        };

        let mut synth = GpuSynth::new(config)
            .map_err(|e| Error::InitFailed(format!("LGS (GPU) 初始化失败: {e}")))?;
        synth
            .load_soundfont(soundfont_path, 0, 0)
            .map_err(|e| Error::InitFailed(format!("LGS (GPU) 音色库加载失败: {e}")))?;

        let playback = AudioPlayback::start(synth)
            .map_err(|e| Error::InitFailed(format!("LGS (GPU) 音频流启动失败: {e}")))?;
        let event_tx = Arc::new(Mutex::new(playback.event_sender()));
        let velocity_filter = Arc::new(AtomicU8::new(options.velocity_filter_threshold));

        let version = format!("lumino-gpu-synth {}", lumino_gpu_synth::VERSION);
        tracing::info!("LGS (GPU): 初始化完成");

        Ok(Self {
            _playback: Arc::new(Mutex::new(playback)),
            event_tx,
            velocity_filter,
            soundfont_path: soundfont_path.to_path_buf(),
            options: options.clone(),
            version,
        })
    }
}

impl Api for Lgs {
    fn version(&self) -> Option<String> {
        Some(self.version.clone())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(Vec::new())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(vec![OutputInfo {
            id: 0,
            name: "LGS (GPU)".to_string(),
        }])
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(LgsOutputConn {
            event_tx: Arc::clone(&self.event_tx),
            velocity_filter: Arc::clone(&self.velocity_filter),
        }))
    }

    fn open_input(
        &self,
        _id: u32,
        _callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        Err(Error::InitFailed(
            "LGS (GPU) does not support MIDI input".into(),
        ))
    }
}

/// LGS (GPU) MIDI 输出连接：把 MIDI 事件转发给 GPU 渲染线程
pub(crate) struct LgsOutputConn {
    event_tx: Arc<Mutex<Option<mpsc::Sender<(u8, MidiEvent)>>>>,
    velocity_filter: Arc<AtomicU8>,
}

impl LgsOutputConn {
    /// 向 GPU 渲染线程发送一个 MIDI 事件；发送器不可用（已停止）时静默丢弃。
    fn send_event(&self, channel: u8, event: MidiEvent) {
        if let Ok(guard) = self.event_tx.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send((channel, event));
            }
        }
    }
}

impl OutputConnection for LgsOutputConn {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        // 响度(力度)过滤：仅对真实按下（vel>0）生效；vel==0 视为释放，不被过滤
        let threshold = self.velocity_filter.load(Ordering::Relaxed);
        if threshold > 0 && vel > 0 && vel <= threshold {
            return Ok(());
        }
        let velocity = if vel == 0 { 1 } else { vel };
        self.send_event(
            channel,
            MidiEvent::NoteOn {
                key: key & MIDI_VALUE_MASK,
                vel: velocity & MIDI_VALUE_MASK,
            },
        );
        Ok(())
    }

    fn note_off(&mut self, ch: u8, key: u8, _vel: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        self.send_event(channel, MidiEvent::NoteOff { key: key & MIDI_VALUE_MASK });
        Ok(())
    }

    fn control_change(&mut self, ch: u8, controller: u8, value: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        self.send_event(channel, MidiEvent::ControlChange { controller, value });
        Ok(())
    }

    fn program_change(&mut self, ch: u8, program: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        self.send_event(channel, MidiEvent::ProgramChange { program });
        Ok(())
    }

    fn pitch_bend(&mut self, ch: u8, value: f32) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        let bend = ((value + 1.0) * 0.5 * f32::from(PITCH_BEND_MAX)).round() as u16;
        self.send_event(channel, MidiEvent::PitchBend { value: bend });
        Ok(())
    }

    fn channel_pressure(&mut self, _ch: u8, _pressure: u8) -> Result<(), Error> {
        // GPU 合成器的 `MidiEvent` 无通道后触变体，忽略（与 xsynth 行为一致，不报错）
        Ok(())
    }

    fn poly_pressure(&mut self, _ch: u8, _key: u8, _pressure: u8) -> Result<(), Error> {
        // GPU 合成器的 `MidiEvent` 无复音后触变体，忽略
        Ok(())
    }

    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        let status = data[0] & 0xF0;
        let channel = (data[0] & 0x0F) as u8;
        let b1 = data[1];
        let b2 = data[2];
        match status {
            0x80 => self.send_event(channel, MidiEvent::NoteOff { key: b1 & MIDI_VALUE_MASK }),
            0x90 => {
                // 响度(力度)过滤：b2>0 的真实音符按下才过滤；b2==0 视为释放
                let threshold = self.velocity_filter.load(Ordering::Relaxed);
                if threshold > 0 && b2 > 0 && b2 <= threshold {
                    // 过轻音符直接丢弃，不发往 GPU 渲染线程
                } else {
                    self.send_event(
                        channel,
                        MidiEvent::NoteOn {
                            key: b1 & MIDI_VALUE_MASK,
                            vel: b2 & MIDI_VALUE_MASK,
                        },
                    );
                }
            }
            0xB0 => self.send_event(channel, MidiEvent::ControlChange {
                controller: b1,
                value: b2,
            }),
            0xC0 => self.send_event(channel, MidiEvent::ProgramChange { program: b1 }),
            0xE0 => {
                let bend = ((b1 as u16) | ((b2 as u16) << 7)) as u16;
                self.send_event(channel, MidiEvent::PitchBend { value: bend });
            }
            // 通道后触(0xD0) / 复音后触(0xA0)：GPU 合成器不支持，忽略以避免噪声报错
            _ => {}
        }
        Ok(())
    }

    fn close(self: Box<Self>) {
        tracing::debug!("LgsOutputConn::close: 关闭连接");
    }
}
