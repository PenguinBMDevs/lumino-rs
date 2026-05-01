//! 参考文档：https://github.com/KeppySoftware/OmniMIDI/blob/3b0b4f2/DeveloperContent/OmniMIDI.cs

use libloading::Library;
use std::sync::Mutex;
use std::{path::Path, sync::Arc};

use crate::{Api, Error, InputInfo, OutputConnection, OutputInfo};

/// KDMAPI 全局实例（单例）
static KDMAPI_INSTANCE: Mutex<Option<Arc<KdmapiInner>>> = Mutex::new(None);

#[derive(thiserror::Error, Debug)]
/// KDMAPI 错误处理
pub enum KdmapiError {
    #[error("not available")] // KDMAPI 未可用
    NotAvailable,
    #[error("failed to initialize stream")] // KDMAPI 初始化流失败
    InitStreamFailed,
    #[error("failed to request version")] // KDMAPI 请求版本失败
    GetVersionFailed,

    #[error("failed to load: {0}")] // KDMAPI 加载失败
    Load(#[from] libloading::Error),
}

/// KDMAPI 错误转换
impl From<libloading::Error> for Error {
    fn from(e: libloading::Error) -> Self {
        Error::InitFailed(e.to_string())
    }
}

/// KDMAPI 符号表
struct Symbols {
    /// `bool ReturnKDMAPIVer(out Int32 Major, out Int32 Minor, out Int32 Build, out Int32 Revision);`
    return_kdmapi_ver: unsafe extern "system" fn(*mut i32, *mut i32, *mut i32, *mut i32) -> bool, // KDMAPI 返回版本
    /// `bool IsKDMAPIAvailable();`
    is_kdmapi_available: unsafe extern "system" fn() -> bool, // KDMAPI 是否可用
    /// `int InitializeKDMAPIStream();`
    initialize_kdmapi_stream: unsafe extern "system" fn() -> i32, // KDMAPI 初始化流
    /// `int TerminateKDMAPIStream();`
    // terminate_kdmapi_stream: unsafe extern "system" fn() -> i32,
    /// `void ResetKDMAPIStream();`
    // reset_kdmapi_stream: unsafe extern "system" fn() -> (),
    /// `uint SendCustomEvent(uint eventtype, uint chan, uint param);`
    // send_custom_event: unsafe extern "system" fn(u32, u32, u32) -> u32,
    /// `uint SendDirectData(uint dwMsg);`
    send_direct_data: unsafe extern "system" fn(u32) -> u32, // KDMAPI 发送直接数据
}

/// KDMAPI 内部结构
pub struct KdmapiInner {
    _lib: Library,
    sym: Arc<Symbols>,
    version: String,
}

/// KDMAPI 实例
pub struct Kdmapi {
    inner: Arc<KdmapiInner>,
}

/// KDMAPI 实例方法
impl Kdmapi {
    pub fn new(path: &Path) -> Result<Self, Error> {
        // 检查是否已经有初始化的实例
        if let Ok(guard) = KDMAPI_INSTANCE.lock()
            && let Some(instance) = guard.as_ref()
        {
            tracing::info!("KDMAPI 实例已存在，重用它");
            return Ok(Self {
                inner: instance.clone(),
            });
        }

        unsafe {
            let lib = Library::new(path)?;
            // 符号的生命周期应该与 `lib` 一样长
            let sym = Arc::new(Symbols {
                // KDMAPI 符号表
                return_kdmapi_ver: *lib.get(b"ReturnKDMAPIVer\0")?, // KDMAPI 返回版本
                is_kdmapi_available: *lib.get(b"IsKDMAPIAvailable\0")?, // KDMAPI 是否可用
                initialize_kdmapi_stream: *lib.get(b"InitializeKDMAPIStream\0")?, // KDMAPI 初始化流
                // terminate_kdmapi_stream: *lib.get(b"TerminateKDMAPIStream\0")?,
                // reset_kdmapi_stream: *lib.get(b"ResetKDMAPIStream\0")?,
                // send_custom_event: *lib.get(b"SendCustomEvent\0")?,
                send_direct_data: *lib.get(b"SendDirectData\0")?, // KDMAPI 发送直接数据
            });

            // KDMAPI 是否可用
            if !(sym.is_kdmapi_available)() {
                return Err(Error::InitFailed(KdmapiError::NotAvailable.to_string()));
            };
            // KDMAPI 初始化流
            if (sym.initialize_kdmapi_stream)() == 0 {
                return Err(Error::InitFailed(KdmapiError::InitStreamFailed.to_string()));
            };

            // KDMAPI 返回版本
            let mut major = 0;
            let mut minor = 0;
            let mut patch = 0;
            let mut rev = 0;
            if !(sym.return_kdmapi_ver)(&mut major, &mut minor, &mut patch, &mut rev) {
                return Err(Error::InitFailed(KdmapiError::GetVersionFailed.to_string()));
            };

            let inner = Arc::new(KdmapiInner {
                _lib: lib,
                sym,
                version: format!("v{major}.{minor}.{patch}.{rev}"),
            });

            // 保存到全局实例
            if let Ok(mut guard) = KDMAPI_INSTANCE.lock() {
                *guard = Some(inner.clone());
            }

            // KDMAPI 版本
            Ok(Self { inner })
        }
    }
}

/// KDMAPI 实现 API 接口
impl Api for Kdmapi {
    fn version(&self) -> Option<String> {
        Some(self.inner.version.clone())
    }
    // KDMAPI 输入端口
    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(Vec::new())
    }
    // KDMAPI 输出端口
    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(Vec::from(&[OutputInfo {
            id: 0,
            name: "Default".into(),
        }]))
    }
    // KDMAPI 打开输出端口
    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(KdmapiOutputConn {
            sym: self.inner.sym.clone(),
        }))
    }
}

/// KDMAPI 输出端口连接
struct KdmapiOutputConn {
    sym: Arc<Symbols>,
}

// KDMAPI 输出端口连接方法
impl KdmapiOutputConn {
    fn send(&mut self, data: &[u8; 3]) -> Result<(), Error> {
        // MIDI 消息格式：低字节在前（小端序）
        // 第1字节: 状态字节 (如 0x90 | channel)
        // 第2字节: 数据1 (如 key)
        // 第3字节: 数据2 (如 velocity)
        // 第4字节: 0
        let word = u32::from_le_bytes([data[0], data[1], data[2], 0]);

        tracing::trace!(
            "KDMAPI: 发送 MIDI 消息 [{:02X}, {:02X}, {:02X}] = 0x{:08X}",
            data[0],
            data[1],
            data[2],
            word
        );

        let result = unsafe { (self.sym.send_direct_data)(word) };

        // 检查结果（某些 KDMAPI 实现会返回非零表示成功）
        if result == 0 {
            tracing::trace!("KDMAPI: 消息发送成功");
        } else {
            tracing::trace!("KDMAPI: 消息发送结果 = {}", result);
        }

        Ok(())
    }
}

// KDMAPI 输出端口连接实现 OutputConnection 接口
impl OutputConnection for KdmapiOutputConn {
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        // 确保通道在有效范围内 (0-15)
        let channel = ch & 0x0F;
        // Note On 状态字节: 0x90 | channel
        let status = 0x90 | channel;

        tracing::debug!(
            "KDMAPI::note_on: raw_ch={}, channel={}, key={}, vel={}",
            ch,
            channel,
            key,
            vel
        );

        // 确保 velocity 不为 0（否则会被视为 note_off）
        let velocity = if vel == 0 { 1 } else { vel };

        self.send(&[status, key & 0x7F, velocity & 0x7F])
    }

    fn note_off(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        // 确保通道在有效范围内 (0-15)
        let channel = ch & 0x0F;
        // Note Off 状态字节: 0x80 | channel
        let status = 0x80 | channel;

        tracing::debug!(
            "KDMAPI::note_off: raw_ch={}, channel={}, key={}, vel={}",
            ch,
            channel,
            key,
            vel
        );

        self.send(&[status, key & 0x7F, vel & 0x7F])
    }

    fn control_change(&mut self, ch: u8, controller: u8, value: u8) -> Result<(), Error> {
        let channel = ch & 0x0F;
        let status = 0xB0 | channel;
        self.send(&[status, controller, value])
    }

    fn program_change(&mut self, ch: u8, program: u8) -> Result<(), Error> {
        let channel = ch & 0x0F;
        let status = 0xC0 | channel;
        self.send(&[status, program, 0])
    }

    fn pitch_bend(&mut self, ch: u8, value: f32) -> Result<(), Error> {
        let channel = ch & 0x0F;
        let status = 0xE0 | channel;
        let bend = ((value + 1.0) * 0.5 * 16383.0).round() as u16;
        let lsb = (bend & 0x7F) as u8;
        let msb = ((bend >> 7) & 0x7F) as u8;
        self.send(&[status, lsb, msb])
    }

    fn channel_pressure(&mut self, ch: u8, pressure: u8) -> Result<(), Error> {
        let channel = ch & 0x0F;
        let status = 0xD0 | channel;
        self.send(&[status, pressure, 0])
    }

    fn poly_pressure(&mut self, ch: u8, key: u8, pressure: u8) -> Result<(), Error> {
        let channel = ch & 0x0F;
        let status = 0xA0 | channel;
        self.send(&[status, key, pressure])
    }

    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        self.send(&data)
    }

    fn close(self: Box<Self>) {
        tracing::debug!("KDMAPI::close: 关闭连接");
        // Kdmapi 不需要显式关闭对等端
    }
}
