pub mod api;

use std::{path::PathBuf, sync::OnceLock};
use api::{
    kdmapi::KdmapiEngine,
    winmm::WinmmEngine,
};
use thiserror::Error;

static ENGINE: OnceLock<Box<dyn MidiEngine>> = OnceLock::new();

#[derive(Debug)]
pub enum MidiEngineConfig {
    Kdmapi { path: PathBuf },
    Winmm { id: u32 },
    /* TODO */
    CoreMidi {},
}

#[derive(Error, Debug)]
pub enum MidiEngineError {
    #[error("failed to load MidiEngine {name}, reason: {reason}")]
    LoadFailed {
        name: String,
        reason: String,
    },
    #[error("MidiEngine {name} already inited")]
    AlreadyInited {
        name: String,
    },
    #[error("failed to initialize MidiEngine {name}, reason: {reason}")]
    InitFailed {
        name: String,
        reason: String,
    },
}

pub trait MidiEngine: Send + Sync {
    fn init(&mut self) -> Result<(), MidiEngineError>;
    fn version(&self) -> Option<String>;
}

fn get_engine<'a>() -> Option<&'a dyn MidiEngine> {
    ENGINE.get().map(|v| &**v)
}

pub fn init(cfg: MidiEngineConfig) -> Result<(), MidiEngineError> {
    use MidiEngineConfig::*;
    let mut engine: Box<dyn MidiEngine> = match cfg {
        Kdmapi { path } => Box::new(KdmapiEngine::new(path)?),
        Winmm { id } => Box::new(WinmmEngine::new(id)?),
        CoreMidi {} => todo!(),
    };
    engine.init()?;

    ENGINE.get_or_init(|| engine);
    Ok(())
}

pub fn version() -> Option<String> {
    get_engine()?.version()
}
