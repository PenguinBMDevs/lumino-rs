use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use xsynth_core::{
    AudioStreamParams, ChannelCount,
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    soundfont::SoundfontBase,
};

use lumino_realtime::{
    RealtimeSynth, SynthEvent, ThreadCount as LuminoThreadCount, XSynthRealtimeConfig,
};

use crate::constants::*;
use crate::soundfont_cache;
use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
};

/// XSynth 运行时统计信息
#[derive(Debug, Clone, Copy, Default)]
pub struct XSynthStats {
    /// 当前活跃 voice 数量
    pub voice_count: u64,
    /// 渲染器平均负载 (0.0 - 1.0)
    pub average_renderer_load: f64,
    /// 缓冲区样本数
    pub buffer_samples: i64,
}

pub struct XSynthOptions {
    pub buffer_ms: f64,
    pub threads: i32,
    pub sample_rate: u32,
    pub fade_out_killing: bool,
    /// 每个键允许的最大同音数（None = 使用 xsynth 默认值 4）
    /// 调高可减少密集钢琴/快速重复音符/拖音过程中的 voice stealing
    /// 最大并发发音数（git 版 xsynth 暂不支持此字段）
    pub max_voices_per_key: Option<usize>,
    /// 全局最大并发 voice 数。超过此值时新 NoteOn 的 voice 创建被静默跳过。
    /// 设置越小则渲染越快（但同一声道的并发发音数越少）。
    /// None = 使用 xsynth 默认值 4096
    pub global_voice_limit: Option<usize>,
}

pub struct XSynth {
    synth: RealtimeSynth,
    sender: crossbeam_channel::Sender<SynthEvent>,
    version: String,
}

impl XSynth {
    pub fn new(soundfont_path: &Path, options: Option<XSynthOptions>) -> Result<Self, Error> {
        tracing::info!("XSynth: 初始化，音色库路径: {:?}", soundfont_path);

        // 检查音色库文件是否存在
        if !soundfont_path.exists() {
            return Err(Error::InitFailed(format!(
                "Soundfont file not found: {:?}",
                soundfont_path
            )));
        }

        // 在打开音频流之前，先用配置的采样率构造 AudioStreamParams
        // 提前加载音色库。这样在音频流启动时，音色库已经就绪，
        // BufferedRenderer 的 render pipeline 能立即产生有效数据，
        // 避免 callback 在 recv() 上阻塞导致 ALSA underrun。
        let sample_rate = options
            .as_ref()
            .map(|o| o.sample_rate)
            .unwrap_or(DEFAULT_SAMPLE_RATE);
        let load_params = AudioStreamParams::new(sample_rate, ChannelCount::Stereo);

        tracing::info!("XSynth: 预加载音色库 (sample_rate={})...", sample_rate);
        let load_start = Instant::now();
        let soundfont = soundfont_cache::load_soundfont_cached(soundfont_path, load_params)
            .map_err(Error::InitFailed)?;
        tracing::info!(
            "XSynth: 音色库加载完成，耗时: {:.2} 秒",
            load_start.elapsed().as_secs_f64()
        );

        // 音色库已就绪，现在打开音频流
        let mut rt_config = XSynthRealtimeConfig::default();
        let requested_sample_rate = sample_rate; // 复用上面已计算的采样率

        if let Some(opt) = options {
            rt_config.render_window_ms = opt.buffer_ms;

            // 解析线程数
            let thread_count = match opt.threads {
                -1 => LuminoThreadCount::None,
                0 => LuminoThreadCount::Auto,
                n if n > 0 => LuminoThreadCount::Manual(n as usize),
                _ => LuminoThreadCount::Auto,
            };
            rt_config.multithreading = thread_count;
            rt_config.channel_init_options.fade_out_killing = opt.fade_out_killing;
            rt_config.channel_init_options.max_voices_per_key = opt.max_voices_per_key;
            rt_config.channel_init_options.global_voice_limit = opt.global_voice_limit;
        }

        let synth = RealtimeSynth::open_with_default_output(rt_config);

        // 注意：lumino-realtime 使用音频设备的原生采样率，配置中的 sample_rate 仅用于音色库预加载
        // 实际采样率由 cpal 决定，可能与请求的不同
        let actual_sample_rate = synth.stream_params().sample_rate;

        // 如果实际采样率与预加载时不一致，必须重新加载音色库。
        // SampleSoundfont::new() 的内部预处理（采样率转换、包络时间等）
        // 与目标 AudioStreamParams.sample_rate 强相关，混用会导致音高/速度错误（跑调）。
        let soundfont = if actual_sample_rate != requested_sample_rate {
            tracing::warn!(
                "XSynth: 请求的采样率 {}Hz 与设备实际采样率 {}Hz 不匹配，重新加载音色库...",
                requested_sample_rate,
                actual_sample_rate
            );

            let actual_params = AudioStreamParams::new(actual_sample_rate, ChannelCount::Stereo);
            let reload_start = Instant::now();
            let reloaded = soundfont_cache::load_soundfont_cached(soundfont_path, actual_params)
                .map_err(Error::InitFailed)?;
            tracing::info!(
                "XSynth: 音色库已按实际采样率 {}Hz 重新加载，耗时: {:.2} 秒",
                actual_sample_rate,
                reload_start.elapsed().as_secs_f64()
            );
            reloaded
        } else {
            tracing::info!(
                "XSynth: 音频流已创建并启动 (sample_rate={}Hz)",
                actual_sample_rate
            );
            soundfont
        };

        // 获取 sender — 在 open 后立即配置通道，确保音色库在 callback 首次触发前就位
        let sender = synth
            .get_sender_ref()
            .cloned()
            .ok_or_else(|| Error::InitFailed("Failed to get event sender".to_string()))?;

        // 配置音色库
        let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![soundfont];
        sender
            .send(SynthEvent::AllChannels(ChannelEvent::Config(
                ChannelConfigEvent::SetSoundfonts(soundfonts),
            )))
            .map_err(|e| Error::InitFailed(format!("Failed to send event: {}", e)))?;

        // 重置所有通道，确保音色库生效
        sender
            .send(SynthEvent::AllChannels(ChannelEvent::Audio(
                ChannelAudioEvent::AllNotesKilled,
            )))
            .map_err(|e| Error::InitFailed(format!("Failed to send event: {}", e)))?;

        sender
            .send(SynthEvent::AllChannels(ChannelEvent::Audio(
                ChannelAudioEvent::ResetControl,
            )))
            .map_err(|e| Error::InitFailed(format!("Failed to send event: {}", e)))?;

        let version = "xsynth (lumino-realtime)".to_string();
        tracing::info!("XSynth: 初始化完成");

        Ok(Self {
            synth,
            sender,
            version,
        })
    }

    /// 获取运行时统计信息
    pub fn stats(&self) -> XSynthStats {
        let stats = self.synth.get_stats();
        XSynthStats {
            voice_count: stats.voice_count,
            average_renderer_load: stats.average_renderer_load,
            buffer_samples: stats.last_samples_after_read,
        }
    }
}

impl Api for XSynth {
    fn version(&self) -> Option<String> {
        Some(self.version.clone())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(Vec::new())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(vec![OutputInfo {
            id: 0,
            name: "XSynth".to_string(),
        }])
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(XSynthOutputConn {
            sender: self.sender.clone(),
        }))
    }

    fn open_input(
        &self,
        _id: u32,
        _callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        Err(Error::InitFailed(
            "XSynth does not support MIDI input".into(),
        ))
    }
}

struct XSynthOutputConn {
    sender: crossbeam_channel::Sender<SynthEvent>,
}

impl XSynthOutputConn {
    /// 非阻塞发送事件 — 渲染线程过载时丢弃事件，永不阻塞调用线程。
    fn try_send_event(&self, event: SynthEvent) {
        if self.sender.try_send(event).is_err() {
            tracing::warn!("XSynthOutputConn: 事件通道已满，丢弃事件（渲染线程过载）");
        }
    }
}

impl OutputConnection for XSynthOutputConn {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;

        let velocity = if vel == 0 { 1 } else { vel };
        self.try_send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                key: key & MIDI_VALUE_MASK,
                vel: velocity & MIDI_VALUE_MASK,
            }),
        ));
        Ok(())
    }

    fn note_off(&mut self, ch: u8, key: u8, _vel: u8) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        self.try_send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                key: key & MIDI_VALUE_MASK,
            }),
        ));
        Ok(())
    }

    fn control_change(&mut self, ch: u8, controller: u8, value: u8) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        self.try_send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                controller, value,
            ))),
        ));
        Ok(())
    }

    fn program_change(&mut self, ch: u8, program: u8) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        self.try_send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(program)),
        ));
        Ok(())
    }

    fn pitch_bend(&mut self, ch: u8, value: f32) -> Result<(), Error> {
        let channel = (ch & MIDI_CHANNEL_MASK) as u32;
        self.try_send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                value,
            ))),
        ));
        Ok(())
    }

    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        let status = data[0] & 0xF0;
        let channel = (data[0] & 0x0F) as u32;
        let b1 = data[1];
        let b2 = data[2];

        match status {
            0x80 => self.try_send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                    key: b1 & MIDI_VALUE_MASK,
                }),
            )),
            0x90 => self.try_send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                    key: b1 & MIDI_VALUE_MASK,
                    vel: b2 & MIDI_VALUE_MASK,
                }),
            )),
            0xB0 => self.try_send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(b1, b2))),
            )),
            0xC0 => self.try_send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(b1)),
            )),
            0xD0 => self.try_send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(0, b1))),
            )),
            0xE0 => {
                let bend = ((b1 as u16) | ((b2 as u16) << 7)) as f32;
                self.try_send_event(SynthEvent::Channel(
                    channel,
                    ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                        bend,
                    ))),
                ));
            }
            _ => {
                return Err(Error::SendFailed(format!(
                    "xsynth 不支持的消息类型: 0x{:02X}",
                    status
                )));
            }
        };
        Ok(())
    }

    fn all_notes_off(&mut self) -> Result<(), Error> {
        self.try_send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::AllNotesOff,
        )));
        Ok(())
    }

    fn reset_control(&mut self) -> Result<(), Error> {
        self.try_send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::ResetControl,
        )));
        Ok(())
    }

    fn close(self: Box<Self>) {
        tracing::debug!("XSynthOutputConn::close: 关闭连接");
    }
}
