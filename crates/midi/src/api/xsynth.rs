use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    soundfont::SoundfontBase,
};
use xsynth_realtime::{RealtimeEventSender, RealtimeSynth, SynthEvent, XSynthRealtimeConfig};

use crate::soundfont_cache;
use crate::{Api, Error, InputInfo, OutputConnection, OutputInfo};

pub struct XSynthOptions {
    pub buffer_ms: f64,
    pub threads: i32,
    pub sample_rate: u32,
    pub fade_out_killing: bool,
    /// 每个键允许的最大同音数（None = 使用 xsynth 默认值 4）
    /// 调高可减少密集钢琴/快速重复音符/拖音过程中的 voice stealing
    pub max_voices_per_key: Option<usize>,
}

pub struct XSynth {
    _synth: RealtimeSynth, // 保持 synth 存活
    sender: RealtimeEventSender,
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

        let mut rt_config = XSynthRealtimeConfig::default();
        if let Some(opt) = options {
            rt_config.render_window_ms = opt.buffer_ms;

            // 解析线程数
            let thread_count = match opt.threads {
                -1 => xsynth_realtime::ThreadCount::None,
                0 => xsynth_realtime::ThreadCount::Auto,
                n if n > 0 => xsynth_realtime::ThreadCount::Manual(n as usize),
                _ => xsynth_realtime::ThreadCount::Auto,
            };
            rt_config.multithreading = thread_count;
            rt_config.channel_init_options.fade_out_killing = opt.fade_out_killing;
            if let Some(mvpk) = opt.max_voices_per_key {
                rt_config.channel_init_options.max_voices_per_key = Some(mvpk);
            }
        }

        let mut synth = RealtimeSynth::open_with_default_output(rt_config)
            .map_err(|e| Error::InitFailed(format!("XSynth 启动失败: {:?}", e)))?;
        tracing::info!("XSynth: 音频流已创建并启动");

        let params = synth.stream_params();
        tracing::info!(
            "XSynth: 音频参数 - sample_rate: {}, channels: {:?}",
            params.sample_rate,
            params.channels
        );

        // 获取 sender 的可变引用
        let sender = synth.get_sender_mut();

        // 加载音色库（使用缓存）
        tracing::info!("XSynth: 正在加载音色库...");
        let start_time = Instant::now();

        let soundfont = soundfont_cache::load_soundfont_cached(soundfont_path, params)
            .map_err(Error::InitFailed)?;

        let elapsed = start_time.elapsed();
        tracing::info!(
            "XSynth: 音色库加载完成，耗时: {:.2} 秒",
            elapsed.as_secs_f64()
        );

        // 设置音色库到所有通道
        let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![soundfont];
        sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
            ChannelConfigEvent::SetSoundfonts(soundfonts),
        )));

        // 重置所有通道，确保音色库生效
        sender.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::AllNotesKilled,
        )));
        sender.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::ResetControl,
        )));

        // 克隆 sender 用于后续使用
        let sender_clone = sender.clone();

        let version = "xsynth (buickmeow fork)".to_string();
        tracing::info!("XSynth: 初始化完成");

        Ok(Self {
            _synth: synth,
            sender: sender_clone,
            version,
        })
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
}

struct XSynthOutputConn {
    sender: RealtimeEventSender,
}

impl OutputConnection for XSynthOutputConn {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        let channel = (ch & 0x0F) as u32;

        tracing::debug!(
            "XSynthOutputConn::note_on: raw_ch={}, channel={}, key={}, vel={}",
            ch,
            channel,
            key,
            vel
        );

        let velocity = if vel == 0 { 1 } else { vel };

        self.sender.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                key: key & 0x7F,
                vel: velocity & 0x7F,
            }),
        ));

        tracing::debug!("XSynthOutputConn::note_on: 事件已发送到通道 {}", channel);
        Ok(())
    }

    fn note_off(&mut self, ch: u8, key: u8, _vel: u8) -> Result<(), Error> {
        let channel = (ch & 0x0F) as u32;

        tracing::debug!(
            "XSynthOutputConn::note_off: raw_ch={}, channel={}, key={}",
            ch,
            channel,
            key
        );

        self.sender.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: key & 0x7F }),
        ));

        tracing::debug!("XSynthOutputConn::note_off: 事件已发送到通道 {}", channel);
        Ok(())
    }

    fn control_change(&mut self, ch: u8, controller: u8, value: u8) -> Result<(), Error> {
        let channel = (ch & 0x0F) as u32;

        tracing::debug!(
            "XSynthOutputConn::control_change: channel={}, controller={}, value={}",
            channel,
            controller,
            value
        );

        self.sender.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                controller,
                value,
            ))),
        ));

        Ok(())
    }

    fn program_change(&mut self, ch: u8, program: u8) -> Result<(), Error> {
        let channel = (ch & 0x0F) as u32;

        tracing::debug!(
            "XSynthOutputConn::program_change: channel={}, program={}",
            channel,
            program
        );

        self.sender.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(program)),
        ));

        Ok(())
    }

    fn pitch_bend(&mut self, ch: u8, value: f32) -> Result<(), Error> {
        let channel = (ch & 0x0F) as u32;

        tracing::debug!(
            "XSynthOutputConn::pitch_bend: channel={}, value={}",
            channel,
            value
        );

        self.sender.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                value,
            ))),
        ));

        Ok(())
    }

    fn channel_pressure(&mut self, _ch: u8, _pressure: u8) -> Result<(), Error> {
        // xsynth 目前不直接支持 channel pressure，忽略
        Ok(())
    }

    fn poly_pressure(&mut self, _ch: u8, _key: u8, _pressure: u8) -> Result<(), Error> {
        // xsynth 目前不直接支持 poly pressure，忽略
        Ok(())
    }

    fn send_raw(&mut self, _data: [u8; 3]) -> Result<(), Error> {
        // xsynth 不支持原始 MIDI 发送
        Ok(())
    }

    fn all_notes_off(&mut self) -> Result<(), Error> {
        // 直接使用 xsynth 的 AllNotesOff，比逐通道发 CC 123 高效
        self.sender
            .send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
                ChannelAudioEvent::AllNotesOff,
            )));
        Ok(())
    }

    fn reset_control(&mut self) -> Result<(), Error> {
        // 直接使用 xsynth 的 ResetControl，比逐通道发 CC 121 高效
        self.sender
            .send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
                ChannelAudioEvent::ResetControl,
            )));
        Ok(())
    }

    fn close(self: Box<Self>) {
        tracing::debug!("XSynthOutputConn::close: 关闭连接");
    }
}
