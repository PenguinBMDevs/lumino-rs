//! Reference: https://github.com/KeppySoftware/OmniMIDI/blob/3b0b4f2/DeveloperContent/OmniMIDI.cs

use std::{path::PathBuf, sync::OnceLock};
use libloading::Library;
use thiserror::Error;

use crate::{MidiEngine, MidiEngineError};

static LIBRARY: OnceLock<Library> = OnceLock::new();
static SYMBOLS: OnceLock<Symbols> = OnceLock::new();

const NAME: &'static str = "KDMAPI";

struct Symbols {
    return_kdmapi_ver: unsafe extern "system" fn (*mut u32, *mut u32, *mut u32, *mut u32) -> i32,
    is_kdmapi_available: unsafe extern "system" fn() -> i32,
    initialize_kdmapi_stream: unsafe extern "system" fn() -> i32,
}

#[derive(Error, Debug)]
enum KdmapiError {
    /* GENERAL start */
    #[error("{NAME} library has not been loaded")]
    LibraryNotLoaded,
    #[error("{NAME} library already loaded")]
    LibraryAlreadyLoaded,
    #[error("{NAME} symbols has not been loaded")]
    SymbolsNotLoaded,
    #[error("{NAME} symbols already loaded")]
    SymbolsAlreadyLoaded,
    /* GENERAL end */

    /* ABI start */
    #[error("{NAME} is not available")]
    NotAvailable,
    #[error("failed to initialize {NAME} stream")]
    InitStreamFailed,
    #[error("failed to request {NAME} version")]
    GetVersionFailed,
    /* ABI end */

    #[error("failed when loading {NAME} library")]
    Load(#[from] libloading::Error)
}

pub struct KdmapiEngine {
    inited: bool,
}

impl KdmapiEngine {
    pub fn new(path: PathBuf) -> Result<Self, MidiEngineError> {
        if let Err(e) = load_library(path) {
            return Err(MidiEngineError::LoadFailed {
                name: NAME.into(),
                reason: e.to_string(),
            });
        }

        Ok(Self {
            inited: false,
        })
    }
    fn init_inner(&self) -> Result<(), KdmapiError> {
        let symbols = get_symbols()?;

        if unsafe {
            (symbols.is_kdmapi_available)() == 0
        } {
            return Err(KdmapiError::NotAvailable);
        };

        if unsafe {
            (symbols.initialize_kdmapi_stream)() == 0
        } {
            return Err(KdmapiError::InitStreamFailed);
        };

        Ok(())
    }
    fn version_inner(&self) -> Result<String, KdmapiError> {
        let mut major = 0;
        let mut minor = 0;
        let mut patch = 0;
        let mut rev = 0;
        let symbols = get_symbols()?;

        if unsafe {
            (symbols.return_kdmapi_ver)(
                &mut major, &mut minor, &mut patch, &mut rev
            ) == 0
        } {
            return Err(KdmapiError::GetVersionFailed);
        };

        Ok(format!("v{major}.{minor}.{patch}.{rev}"))
    }
}

impl MidiEngine for KdmapiEngine {
    fn init(&mut self) -> Result<(), MidiEngineError> {
        if self.inited {
            return Err(MidiEngineError::AlreadyInited {
                name: NAME.into()
            });
        }

        if let Err(e) = self.init_inner() {
            return Err(MidiEngineError::InitFailed {
                name: NAME.into(),
                reason: e.to_string(),
            });
        }

        self.inited = true;

        Ok(())
    }
    fn version(&self) -> Option<String> {
        self.version_inner().ok()
    }
}

fn load_library(path: PathBuf) -> Result<(), KdmapiError> {
    let lib = unsafe { Library::new(path)? };

    let symbols = unsafe {
        Symbols {
            return_kdmapi_ver: *lib.get(b"ReturnKDMAPIVer\0")?,
            is_kdmapi_available: *lib.get(b"IsKDMAPIAvailable\0")?,
            initialize_kdmapi_stream: *lib.get(b"InitializeKDMAPIStream\0")?,
        }
    };

    LIBRARY.set(lib).map_err(|_| KdmapiError::LibraryAlreadyLoaded)?;
    SYMBOLS.set(symbols).map_err(|_| KdmapiError::SymbolsAlreadyLoaded)?;

    Ok(())
}

fn get_symbols() -> Result<&'static Symbols, KdmapiError> {
    SYMBOLS
        .get()
        .ok_or(KdmapiError::SymbolsNotLoaded)
}

fn get_library() -> Result<&'static Library, KdmapiError> {
    LIBRARY
        .get()
        .ok_or(KdmapiError::LibraryNotLoaded)
}
