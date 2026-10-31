//! LGS (GPU) 合成后端：基于 `lumino-gpu-synth` 的 GPU 加速合成器。
//!
//! 与 XSynth 同源的设计：后端持有 `AudioPlayback`（内部自带 cpal 音频设备），
//! 只通过共享的事件发送器把 MIDI 事件转发给渲染线程；音频输出完全由 GPU
//! 合成管线产生。所有输出连接共享同一个事件发送器，因此「内置 MIDI 输出组」
//! 中可创建多个连接到同一渲染线程，无需第二个 GPU 实例。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use lumino_core::{tr, trf};
use lumino_gpu_synth::audio::playback::{
    AudioPlayback, EventSender, GpuBufferStats, PlaybackControl, PlaybackStatsReader,
};
use lumino_gpu_synth::midi::MidiEvent;
use lumino_gpu_synth::{GpuSynth, InterpolationMode, SynthConfig};
use lumino_midi_model::multi_port::channels_for_max_port_clamped;

use crate::api::layout::{LayoutAction, layout_action};
use crate::constants::*;
use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
    PlaybackOutput, SynthControl,
};

mod init;
mod stats;

/// 共享 MIDI 事件注入器（输出连接 → GPU 渲染线程；REND-016 #139 带墙钟时间戳
/// 与发送端准入限速）。
///
/// REF-LGS-012：发送端用「纪元 + 连接内缓存」读取——`send_raw`/`note_on` 在
/// 稳态下**不加锁**（只在管线重建、发送端被热替换时加一次锁）。`set` 先写值、
/// 后递增纪元；输出连接发现纪元变化才重新取一次发送端。
pub(crate) struct SharedEventTx {
    inner: Mutex<Option<EventSender>>,
    epoch: AtomicU64,
}

impl SharedEventTx {
    pub(crate) fn new(sender: Option<EventSender>) -> Self {
        Self {
            inner: Mutex::new(sender),
            epoch: AtomicU64::new(0),
        }
    }

    /// 当前纪元（发送端每次被替换都会 +1）。
    fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    /// 读取当前发送端（克隆；只在纪元变化时调用）。
    fn load(&self) -> Option<EventSender> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 热替换发送端（管线重建）；写值成功后纪元 +1，所有连接下一事件重新读取。
    fn set(&self, sender: Option<EventSender>) {
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = sender;
        self.epoch.fetch_add(1, Ordering::Release);
    }
}

/// LGS (GPU) 后端初始化选项
#[derive(Debug, Clone)]
pub struct LgsOptions {
    /// 渲染采样率（Hz）
    pub sample_rate: u32,
    /// 每块渲染的音频帧数（GPU 一次 dispatch 的帧数）
    pub block_size: usize,
    /// 每个 (通道, 键) 的最大同音数
    pub max_voices_per_key: usize,
    /// 全局并发声部硬上限（0 = 不限制；REND-016 #139 防爆）。
    ///
    /// 超限时按 REND-015 的 fade-steal 语义裁剪最老/最轻的声部组（5ms 淡出），
    /// 新音符优先发声；0 保持黑 MIDI 全量模式（旧行为）。
    pub max_voices: usize,
    /// LGS 防爆闸（发送端软 NPS 闸；REND-016 #139）。
    ///
    /// 开启时：过载（Governor L2+）对 NoteOn 令牌桶限速（突发 50ms 成组放行，
    /// 被拒 NoteOn 的 NoteOff 配对抵消）；关闭时恒放行（过载会周期性静音）。
    pub soft_nps_gate: bool,
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
    event_tx: Arc<SharedEventTx>,
    /// 共享控制发送器（轻量复位/踏板清理；重建时热替换内层发送器）
    control_tx: Arc<Mutex<Option<mpsc::Sender<PlaybackControl>>>>,
    /// 当前合成管线使用的最大 MIDI 端口（REND-002；0 = 单端口）
    midi_max_port: u8,
    /// 响度(力度)过滤阈值（所有输出连接共享，note_on 时实时丢弃过轻音符）
    velocity_filter: Arc<AtomicU8>,
    /// 音色库路径（重建/重初始化时重用）
    soundfont_path: PathBuf,
    options: LgsOptions,
    /// 播放统计读取器（`new` 与端口布局重建时刷新；无锁克隆，读取原子）
    stats: PlaybackStatsReader,
    version: String,
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
            cached_sender: None,
            sender_epoch: u64::MAX,
            batch_scratch: Vec::new(),
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
    event_tx: Arc<SharedEventTx>,
    /// 轻量控制发送器（复位/踏板清理；与 Lgs 共享，重建时内层热替换）
    control_tx: Arc<Mutex<Option<mpsc::Sender<PlaybackControl>>>>,
    velocity_filter: Arc<AtomicU8>,
    /// REF-LGS-012：发送端缓存（`sender_epoch` 与共享纪元一致时直接复用，
    /// 稳态零锁；只有管线重建热替换发送端后才重新取一次）。
    cached_sender: Option<EventSender>,
    sender_epoch: u64,
    /// 批量入口的复用 scratch（避免每批一次分配）。
    batch_scratch: Vec<(u8, MidiEvent)>,
}

impl LgsOutputConn {
    /// 刷新发送端缓存：纪元未变时**不加锁**。
    fn refresh_sender(&mut self) {
        let epoch = self.event_tx.epoch();
        if self.sender_epoch != epoch {
            self.cached_sender = self.event_tx.load();
            self.sender_epoch = epoch;
        }
    }

    /// 向 GPU 渲染线程发送一个 MIDI 事件；发送器不可用（已停止）时静默丢弃。
    ///
    /// REND-016 #139：入队盖墙钟时间戳；NoteOn 在 Overload/Emergency 下受
    /// 发送端准入限速（其余事件全放行）——由 `EventSender` 统一处理。
    ///
    /// REF-LGS-012：不再每事件加锁（`refresh_sender` 稳态只比较纪元）。
    fn send_event(&mut self, channel: u8, event: MidiEvent) {
        self.refresh_sender();
        if let Some(tx) = &self.cached_sender {
            tx.send(channel, event);
        }
    }
}

/// MIDI 线协议 3 字节 → `(全局通道, 事件)`；不支持的消息返回 `None`。
///
/// 逐条（[`LgsOutputConn::send_raw`]）与批量（`send_raw_batch`）共用同一映射，
/// 保证两条路径语义**逐事件一致**（REF-LGS-012 验收）。
fn raw_to_event(data: &[u8; 3], velocity_threshold: u8) -> Option<(u8, MidiEvent)> {
    let status = data[0] & 0xF0;
    let channel = data[0] & 0x0F;
    let (b1, b2) = (data[1], data[2]);
    match status {
        0x80 => Some((
            channel,
            MidiEvent::NoteOff {
                key: b1 & MIDI_VALUE_MASK,
            },
        )),
        0x90 => {
            // 响度(力度)过滤：b2>0 的真实音符按下才过滤；b2==0 视为释放
            if velocity_threshold > 0 && b2 > 0 && b2 <= velocity_threshold {
                None
            } else {
                Some((
                    channel,
                    MidiEvent::NoteOn {
                        key: b1 & MIDI_VALUE_MASK,
                        vel: b2 & MIDI_VALUE_MASK,
                    },
                ))
            }
        }
        0xB0 => Some((
            channel,
            MidiEvent::ControlChange {
                controller: b1,
                value: b2,
            },
        )),
        0xC0 => Some((channel, MidiEvent::ProgramChange { program: b1 })),
        0xE0 => Some((
            channel,
            MidiEvent::PitchBend {
                value: (b1 as u16) | ((b2 as u16) << 7),
            },
        )),
        // 通道后触(0xD0) / 复音后触(0xA0)：GPU 合成器不支持，忽略以避免噪声报错
        _ => None,
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
        self.refresh_sender();
        let threshold = self.velocity_filter.load(Ordering::Relaxed);
        if let Some((channel, event)) = raw_to_event(&data, threshold)
            && let Some(tx) = &self.cached_sender
        {
            tx.send(channel, event);
        }
        Ok(())
    }

    /// REF-LGS-012：批量入口——发送端只解析一次（纪元比较一次），事件逐条入队。
    ///
    /// 与逐条 `send_raw` 的语义完全一致（同一 `raw_to_event` 映射 + 同一
    /// `EventSender` 准入闸），并共用一次墙钟时间戳（见 `EventSender::send_batch`）。
    fn send_raw_batch(&mut self, messages: &[[u8; 3]]) -> Result<(), Error> {
        self.refresh_sender();
        let threshold = self.velocity_filter.load(Ordering::Relaxed);
        let Some(tx) = &self.cached_sender else {
            return Ok(());
        };
        let mut scratch = std::mem::take(&mut self.batch_scratch);
        scratch.clear();
        scratch.extend(
            messages
                .iter()
                .filter_map(|msg| raw_to_event(msg, threshold)),
        );
        tx.send_batch(&scratch);
        self.batch_scratch = scratch;
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
                    return Err(tr!("LGS (GPU): 复位请求发送失败（渲染线程已停止）").into());
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
                self.rebuild_with_options(options)?;
                self.midi_max_port = max_port;
                Ok(())
            }
        }
    }

    /// REND-019 PR-D（§1.5 L2）：TDR 后重建 GPU 合成管线（布局不变的热替换）。
    fn rebuild_gpu_pipeline(&mut self) -> Result<(), String> {
        let options = self.options.clone();
        self.rebuild_with_options(options)
    }

    /// GPU 缓冲状态快照（队列毫秒/负载/声部/欠载）。
    fn gpu_buffer_stats(&self) -> Option<GpuBufferStats> {
        Some(self.buffer_stats())
    }
}

impl Lgs {
    /// 管线重建公共机制（REND-002 端口布局重建 / REND-019 PR-D TDR 重建共用）：
    /// 先 `build_synth` 建新引擎（含音色库加载；**失败即返回、旧管线不动**），
    /// 再停旧流、起新流并热替换事件/控制发送器（既有输出连接自动跟随）；
    /// 仅成功才提交 `options`。
    fn rebuild_with_options(&mut self, options: LgsOptions) -> Result<(), String> {
        let synth = Self::build_synth(&self.soundfont_path, &options)
            .map_err(|e| trf!("重建 GPU 合成引擎失败: {e}", e = e))?;
        let device = crate::audio_devices::resolve_audio_output_device(
            options.audio_output_device.as_deref(),
        );
        {
            let mut old = self._playback.lock().unwrap_or_else(|e| e.into_inner());
            old.stop();
        }
        let playback = AudioPlayback::start(synth, device, options.soft_nps_gate)
            .map_err(|e| trf!("重启 GPU 音频流失败: {e}", e = e))?;
        self.event_tx.set(playback.event_sender());
        *self.control_tx.lock().unwrap_or_else(|e| e.into_inner()) = playback.control_sender();
        // 管线重建后统计读取器随之刷新（旧朗读器随旧流冻结）
        self.stats = playback.stats();
        *self._playback.lock().unwrap_or_else(|e| e.into_inner()) = playback;
        self.options = options;
        Ok(())
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

    /// 清零累计统计（欠载 / 丢弃 NoteOn）：经控制通道下发到渲染线程。
    fn reset_playback_stats(&mut self) -> Result<(), Error> {
        if let Ok(guard) = self.control_tx.lock()
            && let Some(tx) = guard.as_ref()
        {
            let _ = tx.send(PlaybackControl::ResetStats);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
