use crate::{Api, Error, InputInfo, OutputConnection, OutputInfo};

const IDENTIFIER: &str = "com.buickmeow.lumino";

impl From<midir::InitError> for Error {
    fn from(e: midir::InitError) -> Self {
        Error::InitFailed(e.to_string())
    }
}

impl From<midir::ConnectError<midir::MidiOutput>> for Error {
    fn from(e: midir::ConnectError<midir::MidiOutput>) -> Self {
        Error::OpenOutputFailed(e.to_string())
    }
}

pub struct System;

impl System {
    pub fn new() -> Result<Self, Error> {
        let _ = Self::input()?;
        let _ = Self::output()?;
        Ok(Self)
    }

    fn input() -> Result<midir::MidiInput, Error> {
        Ok(midir::MidiInput::new(IDENTIFIER)?)
    }

    fn output() -> Result<midir::MidiOutput, Error> {
        Ok(midir::MidiOutput::new(IDENTIFIER)?)
    }

    fn connect(
        output: midir::MidiOutput,
        port: &midir::MidiOutputPort,
    ) -> Result<midir::MidiOutputConnection, Error> {
        Ok(output.connect(port, IDENTIFIER)?)
    }
}

impl Api for System {
    fn version(&self) -> Option<String> {
        None
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        let input = Self::input()?;
        Ok(input
            .ports()
            .iter()
            .enumerate()
            .map(|(k, v)| InputInfo {
                id: k as u32,
                name: input.port_name(v).unwrap_or_else(|_| "<unknown>".into()),
            })
            .collect())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        let output = Self::output()?;
        Ok(output
            .ports()
            .iter()
            .enumerate()
            .map(|(k, v)| OutputInfo {
                id: k as u32,
                name: output.port_name(v).unwrap_or_else(|_| "<unknown>".into()),
            })
            .collect())
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
        let output = Self::output()?;
        let ports = output.ports();
        let port = ports.get(id as usize).ok_or(Error::DeviceNotFound(id))?;
        let conn = Self::connect(output, port)?;
        Ok(Box::new(SystemOutputConn { conn }))
    }
}

struct SystemOutputConn {
    conn: midir::MidiOutputConnection,
}

impl SystemOutputConn {
    fn send(&mut self, data: &[u8; 3]) -> Result<(), Error> {
        self.conn
            .send(data)
            .map_err(|e| Error::SendFailed(e.to_string()))
    }
}

impl OutputConnection for SystemOutputConn {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        self.send(&[0x90 + ch, key, vel])
    }

    fn note_off(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        self.send(&[0x80 + ch, key, vel])
    }

    fn close(self: Box<Self>) {
        self.conn.close();
    }
}
