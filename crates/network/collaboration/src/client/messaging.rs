//! 消息发送方法

use tracing::{error, trace};

use crate::Result;
use crate::types::{MidiEvent, MousePosition, NoteBatchOperation};

use super::{ClientMessage, CollaborationClient};

impl CollaborationClient {
    /// 发送鼠标位置（同步入队，热路径）
    pub fn send_mouse_position(&self, position: MousePosition) -> Result<()> {
        trace!("发送鼠标位置: x={}, y={}", position.x, position.y);
        let result = self.enqueue_message(ClientMessage::MouseMove { position });
        if let Err(ref e) = result {
            error!("发送鼠标位置失败: {}", e);
        }
        result
    }

    /// 发送音符批量操作（同步入队）
    pub fn send_note_batch(&self, operation: NoteBatchOperation) -> Result<()> {
        self.enqueue_message(ClientMessage::NoteBatch { notes: operation })
    }

    /// 发送 MIDI 事件（同步入队）
    pub fn send_midi_event(&self, event: MidiEvent) -> Result<()> {
        self.enqueue_message(ClientMessage::MidiEvent { event })
    }

    /// 发送工程更新（如音轨变更，同步入队）
    pub fn send_project_update(&self, update: crate::types::ProjectUpdate) -> Result<()> {
        self.enqueue_message(ClientMessage::ProjectUpdate { update })
    }

    /// 发送选择同步（本地选择变更，同步入队）
    pub fn send_selection(&self, selection: serde_json::Value) -> Result<()> {
        self.enqueue_message(ClientMessage::Selection { selection })
    }
}
