//! 消息发送方法

use tracing::{debug, error};

use crate::Result;
use crate::types::{MidiEvent, MousePosition, NoteBatchOperation};

use super::{ClientMessage, CollaborationClient};

impl CollaborationClient {
    /// 发送鼠标位置
    pub async fn send_mouse_position(&self, position: MousePosition) -> Result<()> {
        debug!("发送鼠标位置: x={}, y={}", position.x, position.y);
        let result = self
            .send_message(ClientMessage::MouseMove { position })
            .await;
        if let Err(ref e) = result {
            error!("发送鼠标位置失败: {}", e);
        }
        result
    }

    /// 发送音符批量操作
    pub async fn send_note_batch(&self, operation: NoteBatchOperation) -> Result<()> {
        self.send_message(ClientMessage::NoteBatch { notes: operation })
            .await
    }

    /// 发送 MIDI 事件
    pub async fn send_midi_event(&self, event: MidiEvent) -> Result<()> {
        self.send_message(ClientMessage::MidiEvent { event }).await
    }

    /// 发送工程更新（如音轨变更）
    pub async fn send_project_update(&self, update: crate::types::ProjectUpdate) -> Result<()> {
        self.send_message(ClientMessage::ProjectUpdate { update })
            .await
    }
}
