//! Reference: https://github.com/KeppySoftware/OmniMIDI/blob/3b0b4f2/DeveloperContent/OmniMIDI.cs

use std::{path::Path, sync::Arc};
use libloading::Library;
use thiserror::Error;

use crate::{
    MidiApi,
    MidiError,
    MidiInputInfo,
    MidiOutputInfo,
    MidiOutputConnection,
    MidiMessage,
};

#[derive(Error, Debug)]
pub enum KdmapiError {
    #[error("not available")]
    NotAvailable,
    #[error("failed to initialize stream")]
    InitStreamFailed,
    #[error("failed to request version")]
    GetVersionFailed,

    #[error("failed to load: {0}")]
    Load(#[from] libloading::Error)
}

impl From<libloading::Error> for MidiError {
    fn from(e: libloading::Error) -> Self {
        MidiError::InitFailed(e.to_string())
    }
}

struct Symbols {
    /// `bool ReturnKDMAPIVer(out Int32 Major, out Int32 Minor, out Int32 Build, out Int32 Revision);`
    return_kdmapi_ver: unsafe extern "system" fn (*mut i32, *mut i32, *mut i32, *mut i32) -> bool,
    /// `bool IsKDMAPIAvailable();`
    is_kdmapi_available: unsafe extern "system" fn() -> bool,
    /// `int InitializeKDMAPIStream();`
    initialize_kdmapi_stream: unsafe extern "system" fn() -> i32,
    /// `int TerminateKDMAPIStream();`
    terminate_kdmapi_stream: unsafe extern "system" fn() -> i32,
    /// `void ResetKDMAPIStream();`
    reset_kdmapi_stream: unsafe extern "system" fn() -> (),
    /// `uint SendCustomEvent(uint eventtype, uint chan, uint param);`
    send_custom_event: unsafe extern "system" fn(u32, u32, u32) -> u32,
    /// `uint SendDirectData(uint dwMsg);`
    send_direct_data: unsafe extern "system" fn (u32) -> u32,
}

pub struct KdmapiMidi {
    _lib: Library,
    sym: Arc<Symbols>,
    version: String
}

impl KdmapiMidi {
    pub fn new(path: &Path) -> Result<Self, MidiError> {
        unsafe {
            let lib = Library::new(path)?;
            // Symbols are expected to live as long as `lib` is alive.
            let sym = Arc::new(Symbols {
                return_kdmapi_ver: *lib.get(b"ReturnKDMAPIVer\0")?,
                is_kdmapi_available: *lib.get(b"IsKDMAPIAvailable\0")?,
                initialize_kdmapi_stream: *lib.get(b"InitializeKDMAPIStream\0")?,
                terminate_kdmapi_stream: *lib.get(b"TerminateKDMAPIStream\0")?,
                reset_kdmapi_stream: *lib.get(b"ResetKDMAPIStream\0")?,
                send_custom_event: *lib.get(b"SendCustomEvent\0")?,
                send_direct_data: *lib.get(b"SendDirectData\0")?,
            });

            if !(sym.is_kdmapi_available)() {
                return Err(MidiError::InitFailed(
                    KdmapiError::NotAvailable.to_string()
                ));
            };
            if (sym.initialize_kdmapi_stream)() == 0 {
                return Err(MidiError::InitFailed(
                    KdmapiError::InitStreamFailed.to_string()
                ));
            };

            let mut major = 0;
            let mut minor = 0;
            let mut patch = 0;
            let mut rev = 0;
            if !(sym.return_kdmapi_ver)(
                &mut major, &mut minor, &mut patch, &mut rev
            ) {
                return Err(MidiError::InitFailed(
                    KdmapiError::GetVersionFailed.to_string()
                ));
            };

            Ok(Self {
                _lib: lib,
                sym,
                version: format!("v{major}.{minor}.{patch}.{rev}"),
            })
        }
    }
}


impl MidiApi for KdmapiMidi {
    fn version(&self) -> Option<String> {
        Some(self.version.clone())
    }

    fn inputs(&self) -> Result<Vec<MidiInputInfo>, MidiError> {
        Ok(Vec::new())
    }

    fn outputs(&self) -> Result<Vec<MidiOutputInfo>, MidiError> {
        Ok(Vec::from(&[
            MidiOutputInfo {
                id: 0,
                name: "Default".into()
            }
        ]))
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn MidiOutputConnection>, MidiError> {
        if id != 0 {
            return Err(MidiError::DeviceNotFound(id));
        }
        Ok(Box::new(KdmapiOutputConn {
            sym: self.sym.clone()
        }))
    }
}

struct KdmapiOutputConn {
    sym: Arc<Symbols>,
}

impl MidiOutputConnection for KdmapiOutputConn {
    fn send(&mut self, msg: MidiMessage) -> Result<(), MidiError> {
        let word = msg.0[0] as u32
            | ((msg.0[1] as u32) << 8)
            | ((msg.0[2] as u32) << 16);

        unsafe { (self.sym.send_direct_data)(word) };
        Ok(())
    }

    fn close(self: Box<Self>) {
        // Kdmapi doesn't need explict close peer.
    }
}
