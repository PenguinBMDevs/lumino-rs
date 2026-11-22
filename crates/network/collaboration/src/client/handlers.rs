//! 服务器消息处理器

use std::sync::Arc;

use tracing::{debug, error, trace};

use super::event::CollaborationEvent;
use crate::client::{ClientState, ClientStateCell, EventCallback, ServerMessage};

/// 统一触发协作事件回调（消除各分支重复的 `if let Some(ref cb) = callback` 样板）。
fn emit(callback: &Option<EventCallback>, event: CollaborationEvent) {
    if let Some(cb) = callback {
        cb(event);
    }
}

/// 处理服务器消息
pub async fn handle_server_message(
    text: &str,
    state: &Arc<ClientStateCell>,
    callback: Option<EventCallback>,
) -> Result<(), Box<dyn std::error::Error>> {
    let msg: ServerMessage = serde_json::from_str(text)?;

    match msg {
        ServerMessage::UserJoined { user } => {
            emit(&callback, CollaborationEvent::UserJoined { user });
        }

        ServerMessage::UserLeft { user_id } => {
            emit(&callback, CollaborationEvent::UserLeft { user_id });
        }

        ServerMessage::MouseUpdate {
            user_id,
            username,
            position,
            color,
        } => {
            trace!(
                "收到服务器 MouseUpdate: user_id={}, username={}, x={}, y={}, color={}",
                user_id, username, position.x, position.y, color
            );

            emit(
                &callback,
                CollaborationEvent::MouseUpdate {
                    user_id,
                    position,
                    color,
                    username,
                },
            );
        }

        ServerMessage::NoteBatchUpdate { user_id, operation } => {
            emit(
                &callback,
                CollaborationEvent::NoteBatch { user_id, operation },
            );
        }

        ServerMessage::MidiEventUpdate { user_id, event } => {
            emit(&callback, CollaborationEvent::MidiEvent { user_id, event });
        }

        ServerMessage::MidiEventBatchUpdate { user_id, events } => {
            emit(
                &callback,
                CollaborationEvent::MidiEventBatch { user_id, events },
            );
        }

        ServerMessage::ProjectStateUpdate { user_id, update } => {
            emit(
                &callback,
                CollaborationEvent::ProjectUpdate { user_id, update },
            );
        }

        ServerMessage::Selection { user_id, selection } => {
            emit(
                &callback,
                CollaborationEvent::Selection { user_id, selection },
            );
        }

        ServerMessage::FullSync { users, .. } => {
            emit(&callback, CollaborationEvent::FullSync { users });
        }

        ServerMessage::RoomCreated { room } => {
            state.set(ClientState::InRoom);

            emit(&callback, CollaborationEvent::RoomCreated { room });
        }

        ServerMessage::RoomJoined { room, users, .. } => {
            state.set(ClientState::InRoom);

            emit(&callback, CollaborationEvent::RoomJoined { room, users });
        }

        ServerMessage::Error { error } => {
            error!("服务器错误: {}", error);
            emit(&callback, CollaborationEvent::Error { message: error });
        }

        _ => {
            debug!("收到未处理的服务器消息: type={}", msg.type_name());
        }
    }

    Ok(())
}
