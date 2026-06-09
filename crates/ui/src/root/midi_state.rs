//! MIDI 连接状态管理
//!
//! 从 Root 中提取的 MIDI 连接相关状态，减少 Root 的字段数。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// MIDI 连接状态（从 Root 提取）
pub struct MidiConnectionState {
    /// MIDI 文档引用（用于懒加载非当前音轨的音符，避免全量 preload）
    pub document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    /// MIDI 输入连接（保持打开状态，drop 时自动关闭端口）
    pub input_connection: Option<Box<dyn lumino_midi_io::InputConnection>>,
    /// MIDI 输入数据缓冲区（midir 回调线程写入，UI 线程读取）
    pub input_buffer: Arc<Mutex<VecDeque<Vec<u8>>>>,
    /// MIDI API 引用（用于录制时打开输入端口）
    pub api: Option<Box<dyn lumino_midi_io::Api>>,
}

impl MidiConnectionState {
    pub fn new() -> Self {
        Self {
            document: None,
            input_connection: None,
            input_buffer: Arc::new(Mutex::new(VecDeque::new())),
            api: None,
        }
    }
}
