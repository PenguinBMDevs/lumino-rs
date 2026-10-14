use lumino_core::tr;

use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
    PlaybackOutput, SynthControl,
};

const IDENTIFIER: &str = "com.PenguinBMDevs.lumino";

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

/// 系统 MIDI 后端，基于 midir 访问操作系统 MIDI 设备
///
/// # 惰性初始化
///
/// `new()` **不做任何 WinMM/midir 调用**（零耗时、零副作用）：共享句柄
/// `midi_input` / `midi_output` 在**首次访问时**才创建（`with_input` /
/// `with_output`），创建发生在调用线程上——枚举播表等慢操作应放到后台线程，
/// 避免在 UI 线程触碰 WinMM 客户端创建。
///
/// 连接类操作（`open_output` / `open_input`）每次都自建独立实例，不依赖
/// 上述共享句柄（midir 的 `connect` 需要独立所有权）。
pub struct System {
    midi_input: std::sync::Mutex<Option<midir::MidiInput>>,
    midi_output: std::sync::Mutex<Option<midir::MidiOutput>>,
}

impl System {
    /// 创建惰性句柄：不触碰 WinMM。
    ///
    /// 保留 `Result` 签名与历史调用方（`new_api`）兼容；恒返回 `Ok`。
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            midi_input: std::sync::Mutex::new(None),
            midi_output: std::sync::Mutex::new(None),
        })
    }

    fn with_input<T>(
        &self,
        f: impl FnOnce(&midir::MidiInput) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut guard = self
            .midi_input
            .lock()
            .map_err(|e| Error::InitFailed(format!("MIDI input lock poisoned: {e}")))?;
        if guard.is_none() {
            // 首次访问才创建 WinMM 输入客户端（调用线程承担创建成本）
            *guard = Some(midir::MidiInput::new(IDENTIFIER)?);
        }
        let input = guard
            .as_ref()
            .ok_or_else(|| Error::InitFailed(tr!("MIDI 输入未初始化").into()))?;
        f(input)
    }

    fn with_output<T>(
        &self,
        f: impl FnOnce(&midir::MidiOutput) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut guard = self
            .midi_output
            .lock()
            .map_err(|e| Error::InitFailed(format!("MIDI output lock poisoned: {e}")))?;
        if guard.is_none() {
            // 首次访问才创建 WinMM 输出客户端（调用线程承担创建成本）
            *guard = Some(midir::MidiOutput::new(IDENTIFIER)?);
        }
        let output = guard
            .as_ref()
            .ok_or_else(|| Error::InitFailed(tr!("MIDI 输出未初始化").into()))?;
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

    fn open_output(&self, id: u32) -> Result<Box<dyn PlaybackOutput>, Error> {
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

/// REND-002：显式加入能力扩展（空 impl = 接受默认行为：不支持多端口布局）。
impl SynthControl for System {}

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

/// REND-002：显式加入播放能力扩展（空 impl = 默认：模态 no-op / 16 通道踏板释放）。
impl PlaybackOutput for SystemOutputConn {}

impl InputConnection for SystemInputConn {
    fn close(self: Box<Self>) {
        let _ = self.conn.close();
    }
}
