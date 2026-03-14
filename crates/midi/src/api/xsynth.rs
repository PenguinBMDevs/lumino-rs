use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent},
    soundfont::{SampleSoundfont, SoundfontBase},
};
use xsynth_realtime::{RealtimeEventSender, RealtimeSynth, SynthEvent, XSynthRealtimeConfig};

use crate::{Api, Error, InputInfo, OutputConnection, OutputInfo};

pub struct XSynth {
    _synth: RealtimeSynth, // 保持 synth 存活
    sender: RealtimeEventSender,
    version: String,
}

impl XSynth {
    pub fn new(soundfont_path: &Path) -> Result<Self, Error> {
        tracing::info!("XSynth: 初始化，音色库路径: {:?}", soundfont_path);

        // 检查音色库文件是否存在
        if !soundfont_path.exists() {
            return Err(Error::InitFailed(format!(
                "Soundfont file not found: {:?}",
                soundfont_path
            )));
        }

        let config = XSynthRealtimeConfig::default();
        let mut synth = RealtimeSynth::open_with_default_output(config);
        tracing::info!("XSynth: 音频流已创建并启动");

        let params = synth.stream_params();
        tracing::info!(
            "XSynth: 音频参数 - sample_rate: {}, channels: {:?}",
            params.sample_rate,
            params.channels
        );

        // 加载音色库
        tracing::info!("XSynth: 正在加载音色库...");
        let soundfont = SampleSoundfont::new(soundfont_path, params, Default::default())
            .map_err(|e| Error::InitFailed(format!("Failed to load soundfont: {:?}", e)))?;
        tracing::info!("XSynth: 音色库对象创建成功");

        // 获取 sender 的可变引用并设置音色库
        let sender = synth.get_sender_mut();
        let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(soundfont)];

        tracing::info!("XSynth: 设置音色库到所有通道...");
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

        tracing::info!("XSynth: 音色库配置已发送，等待初始化完成...");

        // 给一点时间让音色库初始化生效
        thread::sleep(Duration::from_millis(100));

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
        // 确保通道在有效范围内 (0-15)
        let channel = (ch & 0x0F) as u32;

        tracing::debug!(
            "XSynthOutputConn::note_on: raw_ch={}, channel={}, key={}, vel={}",
            ch,
            channel,
            key,
            vel
        );

        // 确保 velocity 不为 0（否则会被视为 note_off）
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
        // 确保通道在有效范围内 (0-15)
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

    fn close(self: Box<Self>) {
        tracing::debug!("XSynthOutputConn::close: 关闭连接");
    }
}
