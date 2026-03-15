//! 服务器消息处理器

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info};

use super::event::CollaborationEvent;
use crate::client::{
    ClientConfig, ClientMessage, ClientState, CollaborationClient, CollaborationSession,
    EventCallback, ServerMessage,
};
use crate::types::RemoteUser;
use crate::types::*;

/// 处理服务器消息
pub async fn handle_server_message(
    text: &str,
    state: &Arc<RwLock<ClientState>>,
    session: &Arc<RwLock<CollaborationSession>>,
    callback: Option<EventCallback>,
) -> Result<(), Box<dyn std::error::Error>> {
    let msg: ServerMessage = serde_json::from_str(text)?;

    match msg {
        ServerMessage::UserJoined { user } => {
            let mut sess = session.write().await;
            sess.remote_users.insert(
                user.id.clone(),
                RemoteUser {
                    info: user.clone(),
                    mouse_position: None,
                    last_active: Instant::now(),
                },
            );
            drop(sess);

            if let Some(ref cb) = callback {
                cb(CollaborationEvent::UserJoined { user });
            }
        }

        ServerMessage::UserLeft { user_id } => {
            let mut sess = session.write().await;
            sess.remote_users.remove(&user_id);
            drop(sess);

            if let Some(ref cb) = callback {
                cb(CollaborationEvent::UserLeft { user_id });
            }
        }

        ServerMessage::MouseUpdate {
            user_id,
            username,
            position,
            color,
        } => {
            let mut sess = session.write().await;
            if let Some(user) = sess.remote_users.get_mut(&user_id) {
                user.mouse_position = Some(position.clone());
                user.last_active = Instant::now();
            }
            drop(sess);

            if let Some(ref cb) = callback {
                cb(CollaborationEvent::MouseUpdate {
                    user_id,
                    position,
                    color,
                    username,
                });
            }
        }

        ServerMessage::NoteBatchUpdate { user_id, operation } => {
            if let Some(ref cb) = callback {
                cb(CollaborationEvent::NoteBatch { user_id, operation });
            }
        }

        ServerMessage::MidiEventUpdate { user_id, event } => {
            if let Some(ref cb) = callback {
                cb(CollaborationEvent::MidiEvent { user_id, event });
            }
        }

        ServerMessage::MidiEventBatchUpdate { user_id, events } => {
            if let Some(ref cb) = callback {
                cb(CollaborationEvent::MidiEventBatch { user_id, events });
            }
        }

        ServerMessage::ProjectStateUpdate { user_id, update } => {
            if let Some(ref cb) = callback {
                cb(CollaborationEvent::ProjectUpdate { user_id, update });
            }
        }

        ServerMessage::FullSync { users, .. } => {
            if let Some(ref cb) = callback {
                cb(CollaborationEvent::FullSync { users });
            }
        }

        ServerMessage::RoomCreated { room } => {
            let mut sess = session.write().await;
            sess.current_room = Some(room.clone());
            drop(sess);

            *state.write().await = ClientState::InRoom;

            if let Some(ref cb) = callback {
                cb(CollaborationEvent::RoomCreated { room });
            }
        }

        ServerMessage::RoomJoined { room, users, .. } => {
            let mut sess = session.write().await;
            sess.current_room = Some(room.clone());

            // 添加所有用户
            for user in &users {
                if Some(&user.id) != sess.current_user_id.as_ref() {
                    sess.remote_users.insert(
                        user.id.clone(),
                        RemoteUser {
                            info: user.clone(),
                            mouse_position: None,
                            last_active: Instant::now(),
                        },
                    );
                }
            }
            drop(sess);

            *state.write().await = ClientState::InRoom;

            if let Some(ref cb) = callback {
                cb(CollaborationEvent::RoomJoined { room, users });
            }
        }

        ServerMessage::Error { error } => {
            error!("服务器错误: {}", error);
            if let Some(ref cb) = callback {
                cb(CollaborationEvent::Error { message: error });
            }
        }

        _ => {
            debug!("收到未处理的消息类型");
        }
    }

    Ok(())
}
