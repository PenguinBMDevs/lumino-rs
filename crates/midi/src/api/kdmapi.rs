//! Reference: https://github.com/KeppySoftware/OmniMIDI/blob/3b0b4f2/DeveloperContent/OmniMIDI.cs

use std::{path::Path, sync::Arc};
use libloading::Library;

use crate::{
    Api,
    Error,
    InputInfo,
    OutputInfo,
    OutputConnection,
};

#[derive(thiserror::Error, Debug)]
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

impl From<libloading::Error> for Error {
    fn from(e: libloading::Error) -> Self {
        Error::InitFailed(e.to_string())
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
    // terminate_kdmapi_stream: unsafe extern "system" fn() -> i32,
    /// `void ResetKDMAPIStream();`
    // reset_kdmapi_stream: unsafe extern "system" fn() -> (),
    /// `uint SendCustomEvent(uint eventtype, uint chan, uint param);`
    // send_custom_event: unsafe extern "system" fn(u32, u32, u32) -> u32,
    /// `uint SendDirectData(uint dwMsg);`
    send_direct_data: unsafe extern "system" fn (u32) -> u32,
}

pub struct Kdmapi {
    _lib: Library,
    sym: Arc<Symbols>,
    version: String
}

impl Kdmapi {
    pub fn new(path: &Path) -> Result<Self, Error> {
        unsafe {
            let lib = Library::new(path)?;
            // Symbols are expected to live as long as `lib` is alive.
            let sym = Arc::new(Symbols {
                return_kdmapi_ver: *lib.get(b"ReturnKDMAPIVer\0")?,
                is_kdmapi_available: *lib.get(b"IsKDMAPIAvailable\0")?,
                initialize_kdmapi_stream: *lib.get(b"InitializeKDMAPIStream\0")?,
                // terminate_kdmapi_stream: *lib.get(b"TerminateKDMAPIStream\0")?,
                // reset_kdmapi_stream: *lib.get(b"ResetKDMAPIStream\0")?,
                // send_custom_event: *lib.get(b"SendCustomEvent\0")?,
                send_direct_data: *lib.get(b"SendDirectData\0")?,
            });

            if !(sym.is_kdmapi_available)() {
                return Err(Error::InitFailed(
                    KdmapiError::NotAvailable.to_string()
                ));
            };
            if (sym.initialize_kdmapi_stream)() == 0 {
                return Err(Error::InitFailed(
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
                return Err(Error::InitFailed(
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


impl Api for Kdmapi {
    fn version(&self) -> Option<String> {
        Some(self.version.clone())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(Vec::new())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(Vec::from(&[
            OutputInfo {
                id: 0,
                name: "Default".into()
            }
        ]))
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(KdmapiOutputConn {
            sym: self.sym.clone()
        }))
    }
}

struct KdmapiOutputConn {
    sym: Arc<Symbols>,
}

impl KdmapiOutputConn {
    fn send(&mut self, data: &[u8; 3]) -> Result<(), Error> {
        let word = u32::from_le_bytes([data[0], data[1], data[2], 0]);
        unsafe { (self.sym.send_direct_data)(word) };
        Ok(())
    }
}

impl OutputConnection for KdmapiOutputConn {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        self.send(&[0x90 | ch, key, vel])
    }

    fn note_off(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        self.send(&[0x80 | ch, key, vel])
    }

    fn close(self: Box<Self>) {
        // Kdmapi doesn't need explict close peer.
    }
}
