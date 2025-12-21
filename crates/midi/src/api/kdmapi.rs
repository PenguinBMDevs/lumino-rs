//! We will replace all the error Strings with `thiserror`.
//!
//! Reference: https://github.com/KeppySoftware/OmniMIDI/blob/3b0b4f2/DeveloperContent/OmniMIDI.

use std::{path::Path, sync::OnceLock};
use libloading::Library;

use crate::MidiEngine;

static LIBRARY: OnceLock<Result<Library, libloading::Error>> = OnceLock::new();
static SYMBOLS: OnceLock<Result<Symbols, libloading::Error>> = OnceLock::new();

struct Symbols {
    return_kdmapi_ver: unsafe extern "system" fn (*mut u32, *mut u32, *mut u32, *mut u32) -> i32,
    is_kdmapi_available: unsafe extern "system" fn() -> i32,
    initialize_kdmapi_stream: unsafe extern "system" fn() -> i32,
}

pub struct KdmapiEngine {
    inited: bool,
}

impl KdmapiEngine {
    pub fn new(path: &Path) -> Result<Self, String> {
        load(path).map_err(|e| e.to_string())?;

        Ok(Self {
            inited: false,
        })
    }
}

impl MidiEngine for KdmapiEngine {
    fn init(&mut self) -> Result<(), String> {
        if self.inited {
            return Err("KdmapiError::Alreadyinited".into());
        }

        let symbols = get_symbols()?;

        if unsafe {
            (symbols.is_kdmapi_available)() == 0
        } {
            return Err("KdmapiError::NotAvailable".into());
        };

        if unsafe {
            (symbols.initialize_kdmapi_stream)() == 0
        } {
            return Err("KdmapiError::InitStreamFailed".into());
        };

        self.inited = true;

        Ok(())
    }
    fn version(&self) -> Result<String, String> {
        let mut major = 0;
        let mut minor = 0;
        let mut patch = 0;
        let mut rev = 0;

        if unsafe {
            (get_symbols()?.return_kdmapi_ver)(
                &mut major, &mut minor, &mut patch, &mut rev
            ) == 0
        } {
            return Err("KdmapiError::GetVersionFailed".into());
        }

        Ok(format!("v{major}.{minor}.{patch}.{rev}"))
    }
}

fn load(path: &Path) -> Result<(), &'static libloading::Error> {
    let lib = LIBRARY.get_or_init(|| unsafe {
        Library::new(path)
    }).as_ref()?;

    SYMBOLS.get_or_init(|| unsafe {
        Ok(Symbols {
            return_kdmapi_ver: *lib.get(b"ReturnKDMAPIVer\0")?,
            is_kdmapi_available: *lib.get(b"IsKDMAPIAvailable\0")?,
            initialize_kdmapi_stream: *lib.get(b"InitializeKDMAPIStream\0")?,
        })
    }).as_ref()?;

    Ok(())
}

fn get_symbols() -> Result<&'static Symbols, String> {
    SYMBOLS
        .get()
        .ok_or("KdmapiError::SymbolsNotLoaded")?
        .as_ref()
        .map_err(|e| e.to_string())
}
