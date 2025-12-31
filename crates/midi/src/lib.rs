pub mod api;

use thiserror::Error;

use std::path::PathBuf;

use api::kdmapi::{
    KdmapiMidi
};
use api::native::{
    NativeMidi
};

#[derive(Error, Debug)]
pub enum MidiError {
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
    SendFailed(String)
}

#[derive(Debug, Clone)]
pub struct MidiInputInfo {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct MidiOutputInfo {
    pub id: u32,
    pub name: String,
}

pub trait MidiApi: Send + Sync {
    fn version(&self) -> Option<String>;
    fn inputs(&self) -> Result<Vec<MidiInputInfo>, MidiError>;
    fn outputs(&self) -> Result<Vec<MidiOutputInfo>, MidiError>;
    fn open_output(&self, id: u32) -> Result<Box<dyn MidiOutputConnection>, MidiError>;
}

pub trait MidiOutputConnection: Send {
    fn send(&mut self, msg: MidiMessage) -> Result<(), MidiError>;
    fn close(self: Box<Self>);
}

#[derive(Clone, Copy, Debug)]
pub struct MidiMessage(pub [u8; 3]);

#[derive(Debug)]
pub enum MidiApiKind {
    Kdmapi { path: PathBuf },
    Native,
}

pub fn new_api(kind: &MidiApiKind) -> Result<Box<dyn MidiApi>, MidiError> {
    use MidiApiKind::*;
    let engine: Box<dyn MidiApi> = match kind {
        Kdmapi { path } => Box::new(KdmapiMidi::new(path)?),
        Native => Box::new(NativeMidi::new()?),
    };
    Ok(engine)
}
