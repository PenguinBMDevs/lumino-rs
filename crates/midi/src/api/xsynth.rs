use std::path::Path;
use std::sync::Arc;

use xsynth_core::{
    channel::{ChannelConfigEvent, ChannelEvent},
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

        let config = XSynthRealtimeConfig::default();
        let mut synth = RealtimeSynth::open_with_default_output(config);
        tracing::info!("XSynth: 音频流已创建");

        let params = synth.stream_params();
        tracing::info!("XSynth: 音频参数 - sample_rate: {}", params.sample_rate);

        let soundfont = SampleSoundfont::new(soundfont_path, params, Default::default())
            .map_err(|e| Error::InitFailed(format!("Failed to load soundfont: {:?}", e)))?;
        tracing::info!("XSynth: 音色库加载成功");

        let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(soundfont)];
        tracing::info!("XSynth: 设置音色库到所有通道");
        synth
            .get_sender_mut()
            .send_event(SynthEvent::AllChannels(ChannelEvent::Config(
                ChannelConfigEvent::SetSoundfonts(soundfonts),
            )));
        tracing::info!("XSynth: 音色库配置已发送");

        let version = "xsynth (buickmeow fork)".to_string();
        let sender = synth.get_sender_ref().clone();
        tracing::info!("XSynth: Sender 已获取");

        Ok(Self {
            _synth: synth, // 保持 synth 存活，音频流不会关闭
            sender,
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
        tracing::debug!(
            "XSynthOutputConn::note_on: ch={}, key={}, vel={}",
            ch,
            key,
            vel
        );
        self.sender.send_event(SynthEvent::Channel(
            ch as u32,
            ChannelEvent::Audio(xsynth_core::channel::ChannelAudioEvent::NoteOn { key, vel }),
        ));
        tracing::debug!("XSynthOutputConn::note_on: 发送完成");
        Ok(())
    }

    fn note_off(&mut self, ch: u8, key: u8, _vel: u8) -> Result<(), Error> {
        tracing::debug!("XSynthOutputConn::note_off: ch={}, key={}", ch, key);
        self.sender.send_event(SynthEvent::Channel(
            ch as u32,
            ChannelEvent::Audio(xsynth_core::channel::ChannelAudioEvent::NoteOff { key }),
        ));
        tracing::debug!("XSynthOutputConn::note_off: 发送完成");
        Ok(())
    }

    fn close(self: Box<Self>) {}
}
