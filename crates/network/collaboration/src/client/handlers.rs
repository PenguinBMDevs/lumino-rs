//! 服务器消息处理器

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;
use tracing::{debug, error, info};

use super::event::CollaborationEvent;
use crate::client::{ClientState, CollaborationSession, EventCallback, ServerMessage};
use crate::types::RemoteUser;

/// 统一触发协作事件回调（消除各分支重复的 `if let Some(ref cb) = callback` 样板）。
fn emit(callback: &Option<EventCallback>, event: CollaborationEvent) {
    if let Some(cb) = callback {
        cb(event);
    }
}

/// 构造一个刚加入/同步的远端用户记录（三处重复的统一收口）。
fn new_remote_user(info: crate::types::UserInfo) -> RemoteUser {
    RemoteUser {
        info,
        mouse_position: None,
        last_active: Instant::now(),
    }
}

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
            sess.remote_users
                .insert(user.id.clone(), new_remote_user(user.clone()));
            drop(sess);

            emit(&callback, CollaborationEvent::UserJoined { user });
        }

        ServerMessage::UserLeft { user_id } => {
            let mut sess = session.write().await;
            sess.remote_users.remove(&user_id);
            drop(sess);

            emit(&callback, CollaborationEvent::UserLeft { user_id });
        }

        ServerMessage::MouseUpdate {
            user_id,
            username,
            position,
            color,
        } => {
            info!(
                "收到服务器 MouseUpdate: user_id={}, username={}, x={}, y={}, color={}",
                user_id, username, position.x, position.y, color
            );

            let mut sess = session.write().await;
            if let Some(user) = sess.remote_users.get_mut(&user_id) {
                user.mouse_position = Some(position.clone());
                user.last_active = Instant::now();
            }
            drop(sess);

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

        ServerMessage::FullSync { users, .. } => {
            let mut sess = session.write().await;
            sess.remote_users.clear();
            for user in &users {
                sess.remote_users
                    .insert(user.id.clone(), new_remote_user(user.clone()));
            }
            drop(sess);

            emit(&callback, CollaborationEvent::FullSync { users });
        }

        ServerMessage::RoomCreated { room } => {
            let mut sess = session.write().await;
            sess.current_room = Some(room.clone());
            drop(sess);

            *state.write().await = ClientState::InRoom;

            emit(&callback, CollaborationEvent::RoomCreated { room });
        }

        ServerMessage::RoomJoined { room, users, .. } => {
            let mut sess = session.write().await;
            sess.current_room = Some(room.clone());

            // 添加所有用户
            for user in &users {
                if Some(&user.id) != sess.current_user_id.as_ref() {
                    sess.remote_users
                        .insert(user.id.clone(), new_remote_user(user.clone()));
                }
            }
            drop(sess);

            *state.write().await = ClientState::InRoom;

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
