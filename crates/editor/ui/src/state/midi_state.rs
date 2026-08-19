//! MIDI 连接状态子模块
//!
//! 由 Root 持有，存储 MIDI 连接相关状态。
//!
//! 注意：此模块从 `lumino-ui-core` 迁移而来（ui-core 是 UI 基础层，
//! 不应依赖 playback/midi-io 等业务 crate）。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// MIDI 连接状态（由 Root 持有）
pub struct MidiConnectionState {
    /// MIDI 输入连接，持有连接状态（drop 时自动关闭端口）
    pub input_connection: Option<Box<dyn lumino_midi_io::InputConnection>>,
    /// MIDI 输入数据缓冲区（midir 回调线程写入，UI 线程读取）
    pub input_buffer: Arc<Mutex<VecDeque<Vec<u8>>>>,
    /// MIDI API 句柄，用于枚举端口时保持端口打开
    pub api: Option<Box<dyn lumino_midi_io::Api>>,
}

impl MidiConnectionState {
    /// 创建一个空的 MIDI 连接状态
    pub fn new() -> Self {
        Self {
            input_connection: None,
            input_buffer: Arc::new(Mutex::new(VecDeque::new())),
            api: None,
        }
    }
}

impl Default for MidiConnectionState {
    fn default() -> Self {
        Self::new()
    }
}
