pub mod api;

use std::{path::Path, sync::OnceLock};
use api::{
    kdmapi::KdmapiEngine
};

static ENGINE: OnceLock<Box<dyn MidiEngine>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub enum MidiEngineType {
    Kdmapi,
    Winmm,
    CoreMidi,
}

pub trait MidiEngine: Send + Sync {
    fn init(&mut self) -> Result<(), String>;
    fn version(&self) -> Result<String, String>;
}

fn get_engine<'a>() -> Result<&'a dyn MidiEngine, String> {
    Ok(ENGINE.get()
        .ok_or("Engine not ready")?
        .as_ref())
}

pub fn init(engine: MidiEngineType, path: &Path) -> Result<(), String> {
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

pub fn version() -> Result<String, String> {
    get_engine()?.version()
}
