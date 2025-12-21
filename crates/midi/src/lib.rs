pub mod api;

use std::{path::Path, sync::OnceLock};
use api::{
    kdmapi::KdmapiEngine
};
use thiserror::Error;

static ENGINE: OnceLock<Box<dyn MidiEngine>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub enum MidiEngineType {
    Kdmapi,
    Winmm,
    CoreMidi,
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

pub fn init(engine: MidiEngineType, path: &Path) -> Result<(), MidiEngineError> {
    use MidiEngineType::*;
    let mut engine: Box<dyn MidiEngine> = match engine {
        Kdmapi => Box::new(KdmapiEngine::new(path)?),
        Winmm => todo!(),
        CoreMidi => todo!(),
    };
    engine.init()?;

    ENGINE.get_or_init(|| engine);
    Ok(())
}

pub fn version() -> Option<String> {
    get_engine()?.version()
}
