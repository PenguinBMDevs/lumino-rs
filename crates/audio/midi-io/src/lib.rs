//! MIDI 输入 / 输出 `Api` 抽象层。
//!
//! 提供统一的 MIDI 设备访问接口，支持多种后端（XSynth、KDMAPI、系统 MIDI），
//! 以及基于 xsynth 的音频合成与播放管线，另含内置 UI 音效播放（[`ui_sfx`]）。

pub mod api;
pub mod audio_devices;
pub mod compact;
pub mod constants;
pub mod playback;
pub mod realtime;
pub mod soundfont_cache;
pub mod ui_sfx;

mod profiling;

pub use constants::*;

use thiserror::Error;

use std::path::PathBuf;

use api::Kdmapi;
use api::System;
use api::XSynth;
use api::{Lgs, LgsOptions};

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
    /// 打开指定输出端口并返回连接（含播放输出能力扩展）
    fn open_output(&self, id: u32) -> Result<Box<dyn PlaybackOutput>, Error>;
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

/// 合成器控制能力扩展（REND-002，接口演进 B：基础 `Api` 冻结，能力进扩展 trait）。
///
/// 所有 `Api` 实现需显式 `impl SynthControl`（空 impl = 接受默认行为），
/// 使“后端能力支持矩阵”在代码里可见；仅软件合成后端覆写非通用能力。
pub trait SynthControl: Api {
    /// 设置实时合成管线的 MIDI 端口布局（REND-002）。
    ///
    /// `max_port` 为当前文档使用到的最大 FF 21 端口（0 = 单端口）。支持多端口
    /// 通道空间的后端（XSynth）会据此重建合成管线；默认 no-op（后端不支持多端口，
    /// 保持 16 通道折叠语义，见 C1 决策 a）。
    fn set_midi_port_layout(&mut self, _max_port: u8) -> Result<(), String> {
        Ok(())
    }
}

/// MIDI 输出连接接口
///
/// 支持完整的 MIDI 事件集，包括音符、控制器、音色变换、弯音等。
///
/// 通道参数为 **u16 全局通道**（REND-002 多端口：`port*16 + channel`）。默认实现
/// 按 MIDI 线协议只取低 4 位（折叠到 16 通道），保证普通 MIDI 设备行为不变；
/// 软件合成后端（XSynth，未来的 LGS）可 override 后消费完整全局通道空间。
pub trait OutputConnection: Send {
    /// 发送 Note On（力度为 0 时自动转换为 Note Off）
    ///
    /// `ch` 为全局通道；默认实现折叠到低 4 位。
    fn note_on(&mut self, ch: u16, key: u8, vel: u8) -> Result<(), Error> {
        let channel = (ch & u16::from(MIDI_CHANNEL_MASK)) as u8;
        let velocity = if vel == 0 { 1 } else { vel };
        self.send_raw([
            STATUS_NOTE_ON | channel,
            key & MIDI_VALUE_MASK,
            velocity & MIDI_VALUE_MASK,
        ])
    }

    /// 发送 Note Off
    fn note_off(&mut self, ch: u16, key: u8, vel: u8) -> Result<(), Error> {
        let channel = (ch & u16::from(MIDI_CHANNEL_MASK)) as u8;
        self.send_raw([
            STATUS_NOTE_OFF | channel,
            key & MIDI_VALUE_MASK,
            vel & MIDI_VALUE_MASK,
        ])
    }

    /// 控制器变化（CC）
    fn control_change(&mut self, ch: u16, controller: u8, value: u8) -> Result<(), Error> {
        let channel = (ch & u16::from(MIDI_CHANNEL_MASK)) as u8;
        self.send_raw([
            STATUS_CONTROL_CHANGE | channel,
            controller & MIDI_VALUE_MASK,
            value & MIDI_VALUE_MASK,
        ])
    }

    /// 音色变换（Program Change）
    fn program_change(&mut self, ch: u16, program: u8) -> Result<(), Error> {
        let channel = (ch & u16::from(MIDI_CHANNEL_MASK)) as u8;
        self.send_raw([
            STATUS_PROGRAM_CHANGE | channel,
            program & MIDI_VALUE_MASK,
            0,
        ])
    }

    /// 弯音（Pitch Bend）
    /// value 范围: -1.0 到 1.0
    fn pitch_bend(&mut self, ch: u16, value: f32) -> Result<(), Error> {
        let channel = (ch & u16::from(MIDI_CHANNEL_MASK)) as u8;
        let bend = ((value + 1.0) * 0.5 * f32::from(PITCH_BEND_MAX)).round() as u16;
        let lsb = (bend & u16::from(MIDI_VALUE_MASK)) as u8;
        let msb = ((bend >> 7) & u16::from(MIDI_VALUE_MASK)) as u8;
        self.send_raw([STATUS_PITCH_BEND | channel, lsb, msb])
    }

    /// 通道后触（Channel Aftertouch）
    fn channel_pressure(&mut self, ch: u16, pressure: u8) -> Result<(), Error> {
        let channel = (ch & u16::from(MIDI_CHANNEL_MASK)) as u8;
        self.send_raw([
            STATUS_CHANNEL_PRESSURE | channel,
            pressure & MIDI_VALUE_MASK,
            0,
        ])
    }

    /// 复音后触（Polyphonic Aftertouch）
    fn poly_pressure(&mut self, ch: u16, key: u8, pressure: u8) -> Result<(), Error> {
        let channel = (ch & u16::from(MIDI_CHANNEL_MASK)) as u8;
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
            self.control_change(u16::from(ch), CC_ALL_NOTES_OFF, 0)?;
        }
        Ok(())
    }

    /// 重置所有通道的控制器状态到默认值（弯音居中、CC 归零、踏板释放等）
    /// 默认实现：向所有通道发送 CC 121 (Reset All Controllers)
    fn reset_control(&mut self) -> Result<(), Error> {
        for ch in 0..MIDI_CHANNEL_COUNT {
            self.control_change(u16::from(ch), CC_RESET_ALL_CONTROLLERS, 0)?;
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
    ///
    /// **多端口限制（已知，未修）**：返回数组固定 16 项，只覆盖端口 0 的通道，
    /// 多端口文档中端口 ≥1 的全局通道（16..=255）**不上表**——多端口工程下
    /// 混音台电平表只反映第一个端口。放开需要把返回类型改为按合成通道数动态
    /// 长度（或 `[f32; 256]`）并同步混音台 UI，故当前明确记为已知限制。
    ///
    /// 注意 `set_channel_gain` / `set_channel_pan` **不受**此限制：形参 `u8` 恰好
    /// 覆盖全局通道 `0..=255`（`multi_port::MAX_PORTS = 16` ⇒ 上限 255），
    /// 实现按索引直接访问整张混音表。
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

/// 播放输出能力扩展（REND-002，接口演进 B：基础 `OutputConnection` 冻结）。
///
/// 所有播放输出实现需显式 `impl PlaybackOutput`（空 impl = 接受默认行为）。
/// 基础 trait 只保留 MIDI 线协议语义；非线协议的后端能力（如 xsynth 的通道
/// 配置事件）落在本扩展 trait，外部设备默认 no-op。
pub trait PlaybackOutput: OutputConnection {
    /// 设置某全局通道的打击乐模态（REND-002 方案 B）。
    ///
    /// 仅软件合成后端（XSynth）覆写；外部 MIDI 设备无对应线协议消息，
    /// 默认 no-op（Bank Select 无法表达模态切换，见 fork 约束）。
    fn set_percussion_mode(&mut self, _ch: u16, _on: bool) -> Result<(), Error> {
        Ok(())
    }

    /// 释放所有通道的延音踏板（CC64=0），用于暂停清理。
    ///
    /// 默认实现只覆盖 16 个 MIDI 线通道；多端口场景下软件合成后端（XSynth）
    /// 覆写为 `AllChannels`，确保端口 >0 的全局通道同样释放（REND-002）。
    fn release_all_dampers(&mut self) -> Result<(), Error> {
        for ch in 0..MIDI_CHANNEL_COUNT {
            self.control_change(u16::from(ch), 64, 0)?;
        }
        Ok(())
    }
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
    /// LGS (GPU) 后端
    Lgs {
        /// SoundFont 文件路径
        soundfont_path: PathBuf,
        /// 渲染采样率（Hz）
        sample_rate: u32,
        /// 每块渲染帧数
        block_size: usize,
        /// 每 (通道, 键) 最大同音数
        max_voices_per_key: usize,
        /// 全局并发声部硬上限（0 = 不限制；REND-016 #139 防爆，实时默认 16384）
        max_voices: usize,
        /// LGS 防爆闸（发送端软 NPS 闸开关；REND-016 #139）
        soft_nps_gate: bool,
        /// 是否使用 64 点 sinc 高质量插值
        use_sinc: bool,
        /// 响度(力度)过滤阈值（0=关闭过滤）
        velocity_filter_threshold: u8,
        /// 音频播放输出设备（CPAL 音频设备名；None = 系统默认）
        audio_output_device: Option<String>,
        /// 待播放文档使用到的最大 MIDI 端口（FF 21；0 = 单端口；REND-002）
        midi_max_port: u8,
    },
}

/// 使用默认选项创建指定类型的后端（返回含能力扩展的句柄）
pub fn new_api(kind: &ApiKind) -> Result<Box<dyn SynthControl>, Error> {
    new_api_with_options(kind, None)
}

/// 使用自定义选项创建指定类型的后端（返回含能力扩展的句柄）
pub fn new_api_with_options(
    kind: &ApiKind,

    #[allow(unused_variables)] options: Option<api::xsynth::XSynthOptions>,
) -> Result<Box<dyn SynthControl>, Error> {
    let engine: Box<dyn SynthControl> = match kind {
        ApiKind::XSynth { soundfont_path } => Box::new(XSynth::new(soundfont_path, options)?),
        ApiKind::Kdmapi { path } => Box::new(Kdmapi::new(path)?),
        ApiKind::System => Box::new(System::new()?),
        ApiKind::Lgs {
            soundfont_path,
            sample_rate,
            block_size,
            max_voices_per_key,
            max_voices,
            soft_nps_gate,
            use_sinc,
            velocity_filter_threshold,
            audio_output_device,
            midi_max_port,
        } => Box::new(Lgs::new(
            soundfont_path,
            &LgsOptions {
                sample_rate: *sample_rate,
                block_size: *block_size,
                max_voices_per_key: *max_voices_per_key,
                max_voices: *max_voices,
                soft_nps_gate: *soft_nps_gate,
                use_sinc: *use_sinc,
                velocity_filter_threshold: *velocity_filter_threshold,
                audio_output_device: audio_output_device.clone(),
                midi_max_port: *midi_max_port,
            },
        )?),
    };
    Ok(engine)
}

#[cfg(test)]
mod output_tests;
