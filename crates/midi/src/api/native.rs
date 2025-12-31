use crate::{
    MidiApi,
    MidiError,
    MidiInputInfo,
    MidiOutputInfo,
    MidiOutputConnection,
    MidiMessage,
};

const IDENTIFIER: &'static str = "com.buickmeow.lumino";

impl From<midir::InitError> for MidiError {
    fn from(e: midir::InitError) -> Self {
        MidiError::InitFailed(e.to_string())
    }
}

impl From<midir::ConnectError<midir::MidiOutput>> for MidiError {
    fn from(e: midir::ConnectError<midir::MidiOutput>) -> Self {
        MidiError::OpenOutputFailed(e.to_string())
    }
}

pub struct NativeMidi;

impl NativeMidi {
    pub fn new() -> Result<Self, MidiError> {
        let _ = NativeMidi::input()?;
        let _ = NativeMidi::output()?;
        Ok(Self)
    }

    fn input() -> Result<midir::MidiInput, MidiError> {
        Ok(midir::MidiInput::new(IDENTIFIER)?)
    }

    fn output() -> Result<midir::MidiOutput, MidiError> {
        Ok(midir::MidiOutput::new(IDENTIFIER)?)
    }

    fn connect(port: &midir::MidiOutputPort) -> Result<midir::MidiOutputConnection, MidiError> {
        Ok(NativeMidi::output()?.connect(port, IDENTIFIER)?)
    }
}

impl MidiApi for NativeMidi {
    fn version(&self) -> Option<String> {
        None
    }

    fn inputs(&self) -> Result<Vec<MidiInputInfo>, MidiError> {
        let input = NativeMidi::input()?;
        Ok(input.ports().iter().enumerate().map(|(k, v)| MidiInputInfo {
            id: k as u32,
            name: input.port_name(v).unwrap_or("<unknown>".into())
        }).collect())
    }

    fn outputs(&self) -> Result<Vec<MidiOutputInfo>, MidiError> {
        let output = NativeMidi::output()?;
        Ok(output.ports().iter().enumerate().map(|(k, v)| MidiOutputInfo {
            id: k as u32,
            name: output.port_name(v).unwrap_or("<unknown>".into())
        }).collect())
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn MidiOutputConnection>, MidiError> {
        let output = NativeMidi::output()?;
        let ports = output.ports();
        let port = ports.get(id as usize).ok_or(MidiError::DeviceNotFound(id))?;
        let conn = NativeMidi::connect(port)?;
        Ok(Box::new(NativeOutputConn { conn }))
    }
}

struct NativeOutputConn {
    conn: midir::MidiOutputConnection
}

impl MidiOutputConnection for NativeOutputConn {
    fn send(&mut self, msg: MidiMessage) -> Result<(), MidiError> {
        self.conn.send(&msg.0).map_err(|e| MidiError::SendFailed(e.to_string()))
    }

    fn close(self: Box<Self>) {
        // Midir doesn't need explict close peer.
    }
}
