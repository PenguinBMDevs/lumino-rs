use std::sync::OnceLock;

use thiserror::Error;
use windows::Win32::Media::Audio::{
        HMIDIOUT, MIDI_WAVE_OPEN_TYPE, MIDIOUTCAPSW, midiOutGetDevCapsW, midiOutGetNumDevs, midiOutOpen
    };

use crate::{MidiEngine, MidiEngineError};

static HANDLE: OnceLock<Handle> = OnceLock::new();

// Avoid orphan rule error
struct Handle(HMIDIOUT);
// Avoid `*mut c_void` cannot be shared between threads safely`
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

const NAME: &str = "WinMM";

#[derive(Error, Debug)]
enum WinmmError {
    /* GENERAL start */
    #[error("{NAME} device has not been opened")]
    DeviceNotOpened,
    #[error("{NAME} device {id} already opened")]
    DeviceAlreadyOpened {
        id: u32,
    },
    /* GENERAL end */

    /* MMSYSERR start */
    #[error("{NAME} device {id} is already allocated")]
    Allocated { id: u32 },

    #[error("database error on {NAME} device {id}")]
    BadDB { id: u32 },

    #[error("invalid device id {id} for {NAME}")]
    BadDeviceId { id: u32 },

    #[error("invalid error number for {NAME} device {id}")]
    BadErrNum { id: u32 },

    #[error("failed to delete resource associated with {NAME} device {id}")]
    DeleteError { id: u32 },

    #[error("unspecified general error by {NAME} device {id}")]
    GeneralError { id: u32 },

    #[error("device handle for {NAME} device {id} is busy")]
    HandleBusy { id: u32 },

    #[error("invalid flag used on {NAME} device {id}")]
    InvalidFlag { id: u32 },

    #[error("invalid handle used on {NAME} device {id}")]
    InvalidHandle { id: u32 },

    #[error("invalid alias used on {NAME} device {id}")]
    InvalidAlias { id: u32 },

    #[error("invalid parameter passed to {NAME} device {id}")]
    InvalidParam { id: u32 },

    #[error("required key not found for {NAME} device {id}")]
    KeyNotFound { id: u32 },

    #[error("more data available for {NAME} device {id}")]
    MoreData { id: u32 },

    #[error("no driver available for {NAME} device {id}")]
    NoDriver { id: u32 },

    #[error("driver callback unavailable for {NAME} device {id}")]
    NoDriverCallback { id: u32 },

    #[error("not enough memory available for {NAME} device {id}")]
    NoMemory { id: u32 },

    #[error("{NAME} device {id} is not enabled")]
    NotEnabled { id: u32 },

    #[error("operation not supported on {NAME} device {id}")]
    NotSupported { id: u32 },

    #[error("read error occurred on {NAME} device {id}")]
    ReadError { id: u32 },

    #[error("value not found for {NAME} device {id}")]
    ValNotFound { id: u32 },

    #[error("write error occurred on {NAME} device {id}")]
    WriteError { id: u32 },

    #[error("unknown error returned from {NAME} device {id}")]
    Unknown { id: u32 },
    /* MMSYSERR end */

    /* ABI start */
    #[error("{NAME} reports no MIDI output devices")]
    NoDevices,
    #[error("device id {id} out of range (max {max}) for {NAME}")]
    InvalidDeviceIdRange {
        id: u32,
        max: u32,
    }
    /* ABI end */
}

fn mm_result(code: u32, id: u32) -> Result<(), WinmmError> {
    use windows::Win32::Media::*;
    use WinmmError::*;
    match code {
        MMSYSERR_NOERROR => Ok(()),

        MMSYSERR_ALLOCATED         => Err(Allocated { id }),
        MMSYSERR_BADDB             => Err(BadDB { id }),
        MMSYSERR_BADDEVICEID       => Err(BadDeviceId { id }),
        MMSYSERR_BADERRNUM         => Err(BadErrNum { id }),
        MMSYSERR_DELETEERROR       => Err(DeleteError { id }),
        MMSYSERR_ERROR             => Err(GeneralError { id }),
        MMSYSERR_HANDLEBUSY        => Err(HandleBusy { id }),
        MMSYSERR_INVALFLAG         => Err(InvalidFlag { id }),
        MMSYSERR_INVALHANDLE       => Err(InvalidHandle { id }),
        MMSYSERR_INVALIDALIAS      => Err(InvalidAlias { id }),
        MMSYSERR_INVALPARAM        => Err(InvalidParam { id }),
        MMSYSERR_KEYNOTFOUND       => Err(KeyNotFound { id }),
        MMSYSERR_MOREDATA          => Err(MoreData { id }),
        MMSYSERR_NODRIVER          => Err(NoDriver { id }),
        MMSYSERR_NODRIVERCB        => Err(NoDriverCallback { id }),
        MMSYSERR_NOMEM             => Err(NoMemory { id }),
        MMSYSERR_NOTENABLED        => Err(NotEnabled { id }),
        MMSYSERR_NOTSUPPORTED      => Err(NotSupported { id }),
        MMSYSERR_READERROR         => Err(ReadError { id }),
        MMSYSERR_VALNOTFOUND       => Err(ValNotFound { id }),
        MMSYSERR_WRITEERROR        => Err(WriteError { id }),

        _ => Err(Unknown { id }),
    }
}

pub struct WinmmEngine {
    id: u32,
    opened: bool,
}

impl WinmmEngine {
    pub fn new(id: u32) -> Result<Self, MidiEngineError> {
        Ok(Self {
            id,
            opened: false,
        })
    }
    fn init_inner(&mut self) -> Result<(), WinmmError> {
        let id = self.id;

        let num = unsafe { midiOutGetNumDevs() };
        if num == 0 {
            return Err(WinmmError::NoDevices);
        }
        if id >= num {
            return Err(WinmmError::InvalidDeviceIdRange {
                id,
                max: num - 1,
            });
        }

        let mut caps = MIDIOUTCAPSW::default();
        mm_result(unsafe {
            midiOutGetDevCapsW(
                id as usize,
                &mut caps,
                std::mem::size_of::<MIDIOUTCAPSW>() as u32
            )
        }, id)?;

        let mut raw = HMIDIOUT::default();
        mm_result(unsafe {
            midiOutOpen(
                &mut raw,
                id,
                None,
                None,
                MIDI_WAVE_OPEN_TYPE(0)
            )
        }, id)?;
        HANDLE.set(Handle(raw)).map_err(|_| WinmmError::DeviceAlreadyOpened { id })?;

        Ok(())
    }
}

impl MidiEngine for WinmmEngine {
    fn init(&mut self) -> Result<(), MidiEngineError> {
        if self.opened {
            return Err(MidiEngineError::AlreadyInited {
                name: NAME.into(),
            });
        }

        if let Err(e) = self.init_inner() {
            return Err(MidiEngineError::InitFailed {
                name: NAME.to_string(),
                reason: e.to_string()
            })
        }

        self.opened = true;

        Ok(())
    }
    fn version(&self) -> Option<String> {
        None
    }
}

fn get_handle<'a>() -> Result<&'a Handle, WinmmError> {
    HANDLE
        .get()
        .ok_or(WinmmError::DeviceNotOpened)
}
