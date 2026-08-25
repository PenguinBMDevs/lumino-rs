//! MIDI 输入 / 输出 `Api` 抽象层。
//!
//! 提供统一的 MIDI 设备访问接口，支持多种后端（XSynth、KDMAPI、系统 MIDI），
//! 以及基于 xsynth 的音频合成与播放管线。

pub mod api;
pub mod audio_ring;
pub mod backend;
pub mod compact;
pub mod constants;
pub mod core_backend;
pub mod playback;
pub mod realtime;
pub mod soundfont_cache;

pub use constants::*;

use thiserror::Error;

use std::path::PathBuf;

use api::Kdmapi;
use api::System;
use api::XSynth;

/// MIDI 输入 / 输出错误
#[derive(Error, Debug)]
pub enum Error {
    /// 初始化失败（附原因描述）
    #[error("failed to init: {0}")]
    InitFailed(String),
    /// 获取输入设备列表失败（附原因描述）
    #[error("failed to get inputs: {0}")]
    InputsFailed(String),
    /// 获取输出设备列表失败（附原因描述）
    #[error("failed to get outputs: {0}")]
    OutputsFailed(String),
    /// 未找到指定设备（附设备序号）
    #[error("device#{0} not found.")]
    DeviceNotFound(u32),
    /// 打开输出端口失败（附原因描述）
    #[error("failed to open output: {0}")]
    OpenOutputFailed(String),
    /// 发送 MIDI 信号失败（附原因描述）
    #[error("failed to send MIDI signal: {0}")]
    SendFailed(String),
    /// 打开输入端口失败（附原因描述）
    #[error("failed to open input: {0}")]
    OpenInputFailed(String),
}

/// MIDI 输入设备描述信息
#[derive(Debug, Clone)]
pub struct InputInfo {
    /// 设备序号
    pub id: u32,
    /// 设备名称
    pub name: String,
}

/// MIDI 输出设备描述信息
#[derive(Debug, Clone)]
pub struct OutputInfo {
    /// 设备序号
    pub id: u32,
    /// 设备名称
    pub name: String,
}

/// MIDI 输入连接回调类型
///
/// 参数：时间戳（微秒）、原始 MIDI 数据字节切片
pub type MidiInputCallback = Box<dyn FnMut(u64, &[u8]) + Send>;

/// MIDI 输入连接接口
pub trait InputConnection: Send {
    /// 关闭输入连接
    fn close(self: Box<Self>);
}

/// MIDI 输入 / 输出接口抽象，供各后端实现
pub trait Api: Send + Sync {
    /// 返回后端版本号（若可用）
    fn version(&self) -> Option<String>;
    /// 列出可用的 MIDI 输入设备
    fn inputs(&self) -> Result<Vec<InputInfo>, Error>;
    /// 列出可用的 MIDI 输出设备
    fn outputs(&self) -> Result<Vec<OutputInfo>, Error>;
    /// 打开指定输出端口并返回连接
    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error>;
    /// 打开 MIDI 输入端口
    ///
    /// `callback` 在收到 MIDI 数据时被调用，参数为时间戳（微秒）和原始数据。
    fn open_input(
        &self,
        id: u32,
        callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error>;

    /// 检查音频流是否需要恢复（如音频设备被拔出/更换导致流不可用）。
    ///
    /// 默认实现：不支持流恢复的后端返回 `false`。
    /// XSynth 后端在流自愈失败（如新设备参数与管线不一致）时返回 `true`，
    /// 调用方应随后调用 [`Api::recover_stream`]。
    fn poll_stream_recovery_needed(&self) -> bool {
        false
    }

    /// 恢复音频流：重定向到系统默认输出设备，或全量重建合成管线。
    ///
    /// 默认实现：不支持流恢复的后端返回错误，调用方应仅记录日志。
    fn recover_stream(&mut self) -> Result<(), String> {
        Err("当前后端不支持音频流恢复".to_string())
    }
}

/// MIDI 输出连接接口
///
/// 支持完整的 MIDI 事件集，包括音符、控制器、音色变换、弯音等。
pub trait OutputConnection: Send {
    /// 发送 Note On（力度为 0 时自动转换为 Note Off）
    fn note_on(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        let velocity = if vel == 0 { 1 } else { vel };
        self.send_raw([
            STATUS_NOTE_ON | channel,
            key & MIDI_VALUE_MASK,
            velocity & MIDI_VALUE_MASK,
        ])
    }

    /// 发送 Note Off
    fn note_off(&mut self, ch: u8, key: u8, vel: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        self.send_raw([
            STATUS_NOTE_OFF | channel,
            key & MIDI_VALUE_MASK,
            vel & MIDI_VALUE_MASK,
        ])
    }

    /// 控制器变化（CC）
    fn control_change(&mut self, ch: u8, controller: u8, value: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        self.send_raw([
            STATUS_CONTROL_CHANGE | channel,
            controller & MIDI_VALUE_MASK,
            value & MIDI_VALUE_MASK,
        ])
    }

    /// 音色变换（Program Change）
    fn program_change(&mut self, ch: u8, program: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        self.send_raw([
            STATUS_PROGRAM_CHANGE | channel,
            program & MIDI_VALUE_MASK,
            0,
        ])
    }

    /// 弯音（Pitch Bend）
    /// value 范围: -1.0 到 1.0
    fn pitch_bend(&mut self, ch: u8, value: f32) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        let bend = ((value + 1.0) * 0.5 * f32::from(PITCH_BEND_MAX)).round() as u16;
        let lsb = (bend & u16::from(MIDI_VALUE_MASK)) as u8;
        let msb = ((bend >> 7) & u16::from(MIDI_VALUE_MASK)) as u8;
        self.send_raw([STATUS_PITCH_BEND | channel, lsb, msb])
    }

    /// 通道后触（Channel Aftertouch）
    fn channel_pressure(&mut self, ch: u8, pressure: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        self.send_raw([
            STATUS_CHANNEL_PRESSURE | channel,
            pressure & MIDI_VALUE_MASK,
            0,
        ])
    }

    /// 复音后触（Polyphonic Aftertouch）
    fn poly_pressure(&mut self, ch: u8, key: u8, pressure: u8) -> Result<(), Error> {
        let channel = ch & MIDI_CHANNEL_MASK;
        self.send_raw([
            STATUS_POLY_PRESSURE | channel,
            key & MIDI_VALUE_MASK,
            pressure & MIDI_VALUE_MASK,
        ])
    }

    /// 发送原始 MIDI 消息（3 字节）
    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error>;

    /// 停止所有通道的正在发声的音符（保留 Release 阶段）
    /// 默认实现：向所有通道发送 CC 123 (All Notes Off)
    fn all_notes_off(&mut self) -> Result<(), Error> {
        for ch in 0..MIDI_CHANNEL_COUNT {
            self.control_change(ch, CC_ALL_NOTES_OFF, 0)?;
        }
        Ok(())
    }

    /// 重置所有通道的控制器状态到默认值（弯音居中、CC 归零、踏板释放等）
    /// 默认实现：向所有通道发送 CC 121 (Reset All Controllers)
    fn reset_control(&mut self) -> Result<(), Error> {
        for ch in 0..MIDI_CHANNEL_COUNT {
            self.control_change(ch, CC_RESET_ALL_CONTROLLERS, 0)?;
        }
        Ok(())
    }

    /// 设置某 MIDI 通道的音频域增益（线性，1.0 = 0 dB；负数按 0 处理）。
    ///
    /// 仅音频合成类输出（如 XSynth）实现此能力；纯 MIDI 设备输出默认无操作。
    /// 与 MIDI CC7（音量）解耦：此处是合成管线末端的真正混音增益，
    /// 由混音台 UI 直接驱动，不经过 MIDI 事件流。
    fn set_channel_gain(&mut self, _channel: u8, _gain: f32) -> Result<(), Error> {
        Ok(())
    }

    /// 设置某 MIDI 通道的音频域声像（-1..1，0 = 居中）。
    ///
    /// 仅音频合成类输出实现此能力；纯 MIDI 设备输出默认无操作。
    fn set_channel_pan(&mut self, _channel: u8, _pan: f32) -> Result<(), Error> {
        Ok(())
    }

    /// 读取各 MIDI 通道的实时响度峰值（振幅 0..≈1；超过 1 表示接近/超过削波）。
    ///
    /// 仅音频合成类输出（如 XSynth）实现此能力；纯 MIDI 设备输出默认返回全零。
    /// 索引 = 通道号 0..16。供混音台电平表渲染真实演奏响度。
    fn get_channel_levels(&self) -> [f32; 16] {
        [0.0; 16]
    }

    /// 读取主输出实时响度峰值（振幅 0..≈1）。
    ///
    /// 仅音频合成类输出实现；纯 MIDI 设备输出默认返回 0。
    fn get_master_level(&self) -> f32 {
        0.0
    }

    /// 关闭输出连接
    fn close(self: Box<Self>);
}

/// 后端类型描述
#[derive(Debug)]
pub enum ApiKind {
    /// XSynth 软件合成后端
    XSynth {
        /// SoundFont 文件路径
        soundfont_path: PathBuf,
    },
    /// KDMAPI 后端
    Kdmapi {
        /// 动态库文件路径
        path: PathBuf,
    },
    /// 系统 MIDI 后端
    System,
}

/// 使用默认选项创建指定类型的后端
pub fn new_api(kind: &ApiKind) -> Result<Box<dyn Api>, Error> {
    new_api_with_options(kind, None)
}

/// 使用自定义选项创建指定类型的后端
pub fn new_api_with_options(
    kind: &ApiKind,

    #[allow(unused_variables)] options: Option<api::xsynth::XSynthOptions>,
) -> Result<Box<dyn Api>, Error> {
    let engine: Box<dyn Api> = match kind {
        ApiKind::XSynth { soundfont_path } => Box::new(XSynth::new(soundfont_path, options)?),
        ApiKind::Kdmapi { path } => Box::new(Kdmapi::new(path)?),
        ApiKind::System => Box::new(System::new()?),
    };
    Ok(engine)
}
