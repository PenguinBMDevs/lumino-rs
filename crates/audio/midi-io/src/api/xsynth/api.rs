use std::sync::Arc;

use crate::api::xsynth_output::XSynthOutputConn;
use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputInfo, PlaybackOutput,
    SynthControl,
};

use super::XSynth;

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

    fn open_output(&self, id: u32) -> Result<Box<dyn PlaybackOutput>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(XSynthOutputConn {
            sender: Arc::clone(&self.sender_shared),
            mixer: Arc::clone(&self.mixer_shared),
            master: Arc::clone(&self.master_peak_shared),
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

/// REND-002：软件合成后端的合成器控制能力覆写。
impl SynthControl for XSynth {
    /// 按文档端口布局重建合成管线（0 → Midi/16 通道；否则 Custom）。
    ///
    /// - **同布局**：走轻量复位（`reset_channel_state`，逐通道清打击乐模态 +
    ///   `AllChannels(SystemReset)`，实测 0.01–0.11ms，不重开音频流）；
    /// - **不同布局**：`rebuild_with_layout` 全量重建（104–188ms），仅在新管线
    ///   构建成功后提交布局；失败返回 Err，旧管线继续服务，调用方负责告警。
    fn set_midi_port_layout(&mut self, max_port: u8) -> Result<(), String> {
        if max_port == self.midi_max_port {
            // N-2：布局未变 → 轻量复位（微秒级），不重开音频流；
            // 仍满足“文档切换清掉上一文档 bank/模态”的语义。
            tracing::info!("XSynth: 文档切换，布局未变（max_port={max_port}），轻量复位通道状态");
            return self.reset_channel_state();
        }
        tracing::info!(
            "XSynth: 端口布局 {} -> {max_port}，全量重建合成管线",
            self.midi_max_port
        );
        self.rebuild_with_layout(max_port)
    }
}
