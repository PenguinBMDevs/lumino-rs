use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
};

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

impl From<midir::ConnectError<midir::MidiInput>> for Error {
    fn from(e: midir::ConnectError<midir::MidiInput>) -> Self {
        Error::OpenInputFailed(e.to_string())
    }
}

pub struct System {
    midi_input: std::sync::Mutex<Option<midir::MidiInput>>,
    midi_output: std::sync::Mutex<Option<midir::MidiOutput>>,
}

impl System {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            midi_input: std::sync::Mutex::new(Some(midir::MidiInput::new(IDENTIFIER)?)),
            midi_output: std::sync::Mutex::new(Some(midir::MidiOutput::new(IDENTIFIER)?)),
        })
    }

    fn with_input<T>(
        &self,
        f: impl FnOnce(&midir::MidiInput) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let guard = self.midi_input.lock().map_err(|e| {
            Error::InitFailed(format!("MIDI input lock poisoned: {e}"))
        })?;
        let input = guard
            .as_ref()
            .ok_or_else(|| Error::InitFailed("MIDI 输入未初始化".into()))?;
        f(input)
    }

    fn with_output<T>(
        &self,
        f: impl FnOnce(&midir::MidiOutput) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let guard = self.midi_output.lock().map_err(|e| {
            Error::InitFailed(format!("MIDI output lock poisoned: {e}"))
        })?;
        let output = guard
            .as_ref()
            .ok_or_else(|| Error::InitFailed("MIDI 输出未初始化".into()))?;
        f(output)
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
        self.with_input(|input| {
            Ok(input
                .ports()
                .iter()
                .enumerate()
                .map(|(k, v)| InputInfo {
                    id: k as u32,
                    name: input.port_name(v).unwrap_or_else(|_| "<unknown>".into()),
                })
                .collect())
        })
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        self.with_output(|output| {
            Ok(output
                .ports()
                .iter()
                .enumerate()
                .map(|(k, v)| OutputInfo {
                    id: k as u32,
                    name: output.port_name(v).unwrap_or_else(|_| "<unknown>".into()),
                })
                .collect())
        })
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
        let ports = self.with_output(|output| Ok(output.ports()))?;
        let port = ports.get(id as usize).ok_or(Error::DeviceNotFound(id))?;
        let output = midir::MidiOutput::new(IDENTIFIER)?;
        let conn = Self::connect(output, port)?;
        Ok(Box::new(SystemOutputConn { conn }))
    }

    fn open_input(
        &self,
        id: u32,
        callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        let input = midir::MidiInput::new(IDENTIFIER)?;
        let ports = input.ports();
        let port = ports.get(id as usize).ok_or(Error::DeviceNotFound(id))?;
        let conn = input.connect(
            port,
            IDENTIFIER,
            move |timestamp, data, cb: &mut MidiInputCallback| {
                cb(timestamp, data);
            },
            callback,
        )?;
        Ok(Box::new(SystemInputConn { conn }))
    }
}

struct SystemOutputConn {
    conn: midir::MidiOutputConnection,
}

struct SystemInputConn {
    conn: midir::MidiInputConnection<MidiInputCallback>,
}

impl SystemOutputConn {
    fn send(&mut self, data: &[u8; 3]) -> Result<(), Error> {
        self.conn
            .send(data)
            .map_err(|e| Error::SendFailed(e.to_string()))
    }
}

impl OutputConnection for SystemOutputConn {
    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        self.send(&data)
    }

    fn close(self: Box<Self>) {
        let _ = self.conn.close();
    }
}

impl InputConnection for SystemInputConn {
    fn close(self: Box<Self>) {
        let _ = self.conn.close();
    }
}
