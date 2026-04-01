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

pub trait OutputConnection: Send {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error>;
    fn note_off(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error>;
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
