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

use lumino_gpu_synth::audio::playback::{AudioPlayback, PlaybackControl};
use lumino_gpu_synth::midi::MidiEvent;
use lumino_gpu_synth::{GpuSynth, InterpolationMode, SynthConfig};
use lumino_midi_model::multi_port::channels_for_max_port_clamped;

use crate::api::layout::{LayoutAction, layout_action};
use crate::constants::*;
use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
    PlaybackOutput, SynthControl,
};

/// 共享 MIDI 事件发送器（输出连接 → GPU 渲染线程）。
type SharedEventTx = Arc<Mutex<Option<mpsc::Sender<(u8, MidiEvent)>>>>;

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
    /// 音频播放输出设备（CPAL 音频设备名；None = 使用系统默认输出设备）
    pub audio_output_device: Option<String>,
    /// 待播放文档使用到的最大 MIDI 端口（FF 21；0 = 单端口）。
    ///
    /// REND-002：0 → 16 通道（零行为变化）；否则按
    /// `(min(max_port,15)+1)*16` 开通 GPU 全局通道空间（对齐 XSynth `rt_format`）。
    pub midi_max_port: u8,
}

/// LGS (GPU) 软件合成后端
pub struct Lgs {
    /// 持有 AudioPlayback 以保持 GPU 渲染线程与 cpal 音频流存活
    /// （包在 Mutex 中仅为满足 `Api: Send + Sync`；正常运行无需加锁访问）
    _playback: Arc<Mutex<AudioPlayback>>,
    /// 共享事件发送器（所有输出连接通过它向渲染线程转发 MIDI 事件）
    event_tx: SharedEventTx,
    /// 共享控制发送器（轻量复位/踏板清理；重建时热替换内层发送器）
    control_tx: Arc<Mutex<Option<mpsc::Sender<PlaybackControl>>>>,
    /// 当前合成管线使用的最大 MIDI 端口（REND-002；0 = 单端口）
    midi_max_port: u8,
    /// 响度(力度)过滤阈值（所有输出连接共享，note_on 时实时丢弃过轻音符）
    velocity_filter: Arc<AtomicU8>,
    /// 音色库路径（重建/重初始化时重用）
    soundfont_path: PathBuf,
    options: LgsOptions,
    version: String,
}

impl Lgs {
    /// 构建 GPU 合成引擎（含音色库加载；不涉及音频流）。
    ///
    /// `new` 与 `set_midi_port_layout` 重建共用；端口布局决定
    /// `SynthConfig.midi_channels`（对齐 XSynth `rt_format`）。
    fn build_synth(soundfont_path: &Path, options: &LgsOptions) -> Result<GpuSynth, Error> {
        let config = SynthConfig {
            sample_rate: options.sample_rate,
            block_size: options.block_size,
            max_voices_per_key: options.max_voices_per_key,
            midi_channels: channels_for_max_port_clamped(options.midi_max_port) as usize,
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
        Ok(synth)
    }

    /// 使用指定音色库路径与选项创建 LGS 后端
    pub fn new(soundfont_path: &Path, options: &LgsOptions) -> Result<Self, Error> {
        tracing::info!("LGS (GPU): 初始化，音色库路径: {:?}", soundfont_path);

        if !soundfont_path.exists() {
            return Err(Error::InitFailed(format!(
                "Soundfont file not found: {:?}",
                soundfont_path
            )));
        }

        let synth = Self::build_synth(soundfont_path, options)?;

        // 解析音频播放输出设备：指定设备有效则对其打开流，
        // 否则回退到系统默认输出设备。
        let device = crate::audio_devices::resolve_audio_output_device(
            options.audio_output_device.as_deref(),
        );
        let playback = AudioPlayback::start(synth, device)
            .map_err(|e| Error::InitFailed(format!("LGS (GPU) 音频流启动失败: {e}")))?;
        let event_tx = Arc::new(Mutex::new(playback.event_sender()));
        let control_tx = Arc::new(Mutex::new(playback.control_sender()));
        let velocity_filter = Arc::new(AtomicU8::new(options.velocity_filter_threshold));

        let version = format!("lumino-gpu-synth {}", lumino_gpu_synth::VERSION);
        tracing::info!(
            "LGS (GPU): 初始化完成（midi_max_port={}，midi_channels={}）",
            options.midi_max_port,
            channels_for_max_port_clamped(options.midi_max_port)
        );

        Ok(Self {
            _playback: Arc::new(Mutex::new(playback)),
            event_tx,
            control_tx,
            midi_max_port: options.midi_max_port,
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

    fn open_output(&self, id: u32) -> Result<Box<dyn PlaybackOutput>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(LgsOutputConn {
            event_tx: Arc::clone(&self.event_tx),
            control_tx: Arc::clone(&self.control_tx),
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
    event_tx: SharedEventTx,
    /// 轻量控制发送器（复位/踏板清理；与 Lgs 共享，重建时内层热替换）
    control_tx: Arc<Mutex<Option<mpsc::Sender<PlaybackControl>>>>,
    velocity_filter: Arc<AtomicU8>,
}

impl LgsOutputConn {
    /// 向 GPU 渲染线程发送一个 MIDI 事件；发送器不可用（已停止）时静默丢弃。
    fn send_event(&self, channel: u8, event: MidiEvent) {
        if let Ok(guard) = self.event_tx.lock()
            && let Some(tx) = guard.as_ref()
        {
            let _ = tx.send((channel, event));
        }
    }
}

impl OutputConnection for LgsOutputConn {
    // REND-002 实时多端口：直接消费 u16 全局通道（port*16+ch ≤ 255），不再
    // 4bit 折叠；越界（配置不同步）由引擎侧丢弃并告警。
    fn note_on(&mut self, ch: u16, key: u8, vel: u8) -> Result<(), Error> {
        let channel = ch.min(255) as u8;
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

    fn note_off(&mut self, ch: u16, key: u8, _vel: u8) -> Result<(), Error> {
        let channel = ch.min(255) as u8;
        self.send_event(
            channel,
            MidiEvent::NoteOff {
                key: key & MIDI_VALUE_MASK,
            },
        );
        Ok(())
    }

    fn control_change(&mut self, ch: u16, controller: u8, value: u8) -> Result<(), Error> {
        let channel = ch.min(255) as u8;
        self.send_event(channel, MidiEvent::ControlChange { controller, value });
        Ok(())
    }

    fn program_change(&mut self, ch: u16, program: u8) -> Result<(), Error> {
        let channel = ch.min(255) as u8;
        self.send_event(channel, MidiEvent::ProgramChange { program });
        Ok(())
    }

    fn pitch_bend(&mut self, ch: u16, value: f32) -> Result<(), Error> {
        let channel = ch.min(255) as u8;
        let bend = ((value + 1.0) * 0.5 * f32::from(PITCH_BEND_MAX)).round() as u16;
        self.send_event(channel, MidiEvent::PitchBend { value: bend });
        Ok(())
    }

    fn channel_pressure(&mut self, _ch: u16, _pressure: u8) -> Result<(), Error> {
        // GPU 合成器的 `MidiEvent` 无通道后触变体，忽略（与 xsynth 行为一致，不报错）
        Ok(())
    }

    fn poly_pressure(&mut self, _ch: u16, _key: u8, _pressure: u8) -> Result<(), Error> {
        // GPU 合成器的 `MidiEvent` 无复音后触变体，忽略
        Ok(())
    }

    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        let status = data[0] & 0xF0;
        let channel = data[0] & 0x0F;
        let b1 = data[1];
        let b2 = data[2];
        match status {
            0x80 => self.send_event(
                channel,
                MidiEvent::NoteOff {
                    key: b1 & MIDI_VALUE_MASK,
                },
            ),
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
            0xB0 => self.send_event(
                channel,
                MidiEvent::ControlChange {
                    controller: b1,
                    value: b2,
                },
            ),
            0xC0 => self.send_event(channel, MidiEvent::ProgramChange { program: b1 }),
            0xE0 => {
                let bend = (b1 as u16) | ((b2 as u16) << 7);
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

/// REND-002 实时多端口：按文档端口布局重建 GPU 合成管线（对齐 XSynth）。
impl SynthControl for Lgs {
    /// - 同布局：轻量复位（`AudioPlayback::reset_synth_state`，不重开音频流）；
    /// - 异布局：先构建新 `GpuSynth`（含音色库加载，失败即返回、旧管线不动），
    ///   再停旧流、起新流并热替换事件/控制发送器（已创建的输出连接自动跟随）；
    /// - 仅成功才提交布局。
    fn set_midi_port_layout(&mut self, max_port: u8) -> Result<(), String> {
        match layout_action(self.midi_max_port, max_port) {
            LayoutAction::LightReset => {
                tracing::info!(
                    "LGS (GPU): 文档切换，布局未变（max_port={max_port}），轻量复位通道状态"
                );
                let playback = self._playback.lock().unwrap_or_else(|e| e.into_inner());
                if !playback.reset_synth_state() {
                    return Err("LGS (GPU): 复位请求发送失败（渲染线程已停止）".into());
                }
                Ok(())
            }
            LayoutAction::Rebuild => {
                tracing::info!(
                    "LGS (GPU): 端口布局 {} -> {max_port}，全量重建合成管线",
                    self.midi_max_port
                );
                let mut options = self.options.clone();
                options.midi_max_port = max_port;
                let synth = Self::build_synth(&self.soundfont_path, &options)
                    .map_err(|e| format!("重建 GPU 合成引擎失败: {e}"))?;
                let device = crate::audio_devices::resolve_audio_output_device(
                    options.audio_output_device.as_deref(),
                );
                {
                    let mut old = self._playback.lock().unwrap_or_else(|e| e.into_inner());
                    old.stop();
                }
                let playback = AudioPlayback::start(synth, device)
                    .map_err(|e| format!("重启 GPU 音频流失败: {e}"))?;
                *self.event_tx.lock().unwrap_or_else(|e| e.into_inner()) = playback.event_sender();
                *self.control_tx.lock().unwrap_or_else(|e| e.into_inner()) =
                    playback.control_sender();
                *self._playback.lock().unwrap_or_else(|e| e.into_inner()) = playback;
                self.options = options;
                self.midi_max_port = max_port;
                Ok(())
            }
        }
    }
}

/// REND-002 实时多端口：播放能力扩展覆写。
impl PlaybackOutput for LgsOutputConn {
    /// GPU 打击乐语义在 #107（REND-013，单 SF2 多 preset 路由）实现前保持
    /// no-op；显式覆写以对齐 XSynth 的能力矩阵（而非隐式默认）。
    fn set_percussion_mode(&mut self, _ch: u16, _on: bool) -> Result<(), Error> {
        Ok(())
    }

    /// 释放全部通道的延音踏板（对齐 XSynth `AllChannels` 语义）：经控制通道
    /// 一次性下发引擎级命令，不逐通道发 MIDI 事件（全局通道数可达 256）。
    fn release_all_dampers(&mut self) -> Result<(), Error> {
        if let Ok(guard) = self.control_tx.lock()
            && let Some(tx) = guard.as_ref()
        {
            let _ = tx.send(PlaybackControl::ReleaseAllDampers);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造只含发送通道的连接（不启动 GPU/音频设备）。
    fn conn() -> (
        LgsOutputConn,
        mpsc::Receiver<(u8, MidiEvent)>,
        mpsc::Receiver<PlaybackControl>,
    ) {
        let (event_tx, event_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel();
        (
            LgsOutputConn {
                event_tx: Arc::new(Mutex::new(Some(event_tx))),
                control_tx: Arc::new(Mutex::new(Some(control_tx))),
                velocity_filter: Arc::new(AtomicU8::new(0)),
            },
            event_rx,
            control_rx,
        )
    }

    /// REND-002 实时多端口：全局通道（16..=255）必须原样透传，不得 4bit 折叠。
    #[test]
    fn global_channel_passes_through_without_folding() {
        let (mut c, rx, _crx) = conn();
        c.note_on(16, 60, 100).expect("note_on");
        c.control_change(31, 7, 127).expect("cc");
        let (ch1, ev1) = rx.recv().expect("note_on 事件");
        assert_eq!(ch1, 16, "全局通道 16 不得折叠到 0");
        assert!(matches!(ev1, MidiEvent::NoteOn { key: 60, vel: 100 }));
        let (ch2, _) = rx.recv().expect("cc 事件");
        assert_eq!(ch2, 31);
    }

    /// 踏板清理走控制通道（引擎级 AllChannels 等价语义，不逐通道发事件）。
    #[test]
    fn release_all_dampers_goes_through_control_channel() {
        let (mut c, _rx, crx) = conn();
        c.release_all_dampers().expect("release");
        assert!(matches!(crx.recv(), Ok(PlaybackControl::ReleaseAllDampers)));
    }
}
