pub mod api;
pub mod soundfont_cache;

use thiserror::Error;

use std::path::PathBuf;

use api::Kdmapi;
use api::System;
use api::XSynth;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to init: {0}")]
    InitFailed(String),
    #[error("failed to get inputs: {0}")]
    InputsFailed(String),
    #[error("failed to get outputs: {0}")]
    OutputsFailed(String),
    #[error("device#{0} not found.")]
    DeviceNotFound(u32),
    #[error("failed to open output: {0}")]
    OpenOutputFailed(String),
    #[error("failed to send MIDI signal: {0}")]
    SendFailed(String),
}

#[derive(Debug, Clone)]
pub struct InputInfo {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct OutputInfo {
    pub id: u32,
    pub name: String,
}

pub trait Api: Send + Sync {
    fn version(&self) -> Option<String>;
    fn inputs(&self) -> Result<Vec<InputInfo>, Error>;
    fn outputs(&self) -> Result<Vec<OutputInfo>, Error>;
    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error>;
}

/// MIDI 输出连接接口
///
/// 支持完整的 MIDI 事件集，包括音符、控制器、音色变换、弯音等。
pub trait OutputConnection: Send {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error>;
    fn note_off(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error>;

    /// 控制器变化（CC）
    fn control_change(&mut self, ch: u8, controller: u8, value: u8) -> Result<(), Error>;

    /// 音色变换（Program Change）
    fn program_change(&mut self, ch: u8, program: u8) -> Result<(), Error>;

    /// 弯音（Pitch Bend）
    /// value 范围: -1.0 到 1.0
    fn pitch_bend(&mut self, ch: u8, value: f32) -> Result<(), Error>;

    /// 通道后触（Channel Aftertouch）
    fn channel_pressure(&mut self, ch: u8, pressure: u8) -> Result<(), Error>;

    /// 复音后触（Polyphonic Aftertouch）
    fn poly_pressure(&mut self, ch: u8, key: u8, pressure: u8) -> Result<(), Error>;

    /// 发送原始 MIDI 消息（3 字节）
    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error>;

    /// 停止所有通道的正在发声的音符（保留 Release 阶段）
    /// 默认实现：向所有通道发送 CC 123 (All Notes Off)
    fn all_notes_off(&mut self) -> Result<(), Error> {
        for ch in 0..16 {
            let _ = self.control_change(ch, 123, 0);
        }
        Ok(())
    }

    /// 重置所有通道的控制器状态到默认值（弯音居中、CC 归零、踏板释放等）
    /// 默认实现：向所有通道发送 CC 121 (Reset All Controllers)
    fn reset_control(&mut self) -> Result<(), Error> {
        for ch in 0..16 {
            let _ = self.control_change(ch, 121, 0);
        }
        Ok(())
    }

    fn close(self: Box<Self>);
}

#[derive(Debug)]
pub enum ApiKind {
    XSynth { soundfont_path: PathBuf },
    Kdmapi { path: PathBuf },
    System,
}

pub fn new_api(kind: &ApiKind) -> Result<Box<dyn Api>, Error> {
    new_api_with_options(kind, None)
}

pub fn new_api_with_options(
    kind: &ApiKind,
    options: Option<api::xsynth::XSynthOptions>,
) -> Result<Box<dyn Api>, Error> {
    let engine: Box<dyn Api> = match kind {
        ApiKind::XSynth { soundfont_path } => Box::new(XSynth::new(soundfont_path, options)?),
        ApiKind::Kdmapi { path } => Box::new(Kdmapi::new(path)?),
        ApiKind::System => Box::new(System::new()?),
    };
    Ok(engine)
}
