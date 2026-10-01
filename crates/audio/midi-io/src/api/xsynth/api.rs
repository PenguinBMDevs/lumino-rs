use std::sync::Arc;

use crate::api::xsynth_output::XSynthOutputConn;
use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
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

    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
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

    /// REND-002：按文档端口布局重建合成管线（0 → Midi/16 通道；否则 Custom）。
    ///
    /// `rebuild_with_layout` 仅在新管线构建成功后提交布局；失败返回 Err，
    /// 旧管线继续服务，调用方负责告警。
    fn set_midi_port_layout(&mut self, max_port: u8) -> Result<(), String> {
        if self.midi_max_port == max_port {
            return Ok(());
        }
        tracing::info!(
            "XSynth: 端口布局变化 {} -> {}，重建合成管线",
            self.midi_max_port,
            max_port
        );
        self.rebuild_with_layout(max_port)
    }
}
