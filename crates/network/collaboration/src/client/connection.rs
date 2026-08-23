//! WebSocket 连接和认证

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc};
use tokio::time::interval;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::protocol::Message,
};
use tracing::{debug, error, info, trace};

use crate::Result;

use crate::types::RoomInfo;

use super::handlers::handle_server_message;
use super::{ClientMessage, ClientState, CollaborationClient, CollaborationEvent, ServerMessage};

/// WebSocket 流类型
pub(super) type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub(super) type WsSink = futures::stream::SplitSink<WsStream, Message>;
pub(super) type WsStreamRead = futures::stream::SplitStream<WsStream>;

impl CollaborationClient {
    /// 使用 roomId 连接 WebSocket
    pub(super) async fn connect_with_room_id(&mut self, room_id: &str) -> Result<()> {
        let ws_url = self.build_websocket_url(room_id);
        tracing::debug!("连接到 WebSocket: {}", ws_url);

        self.state.set(ClientState::Connecting);

        let ws_stream = self.connect_websocket_with_timeout(&ws_url).await?;

        info!("WebSocket 连接成功");
        let (write, read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));
        self.state.set(ClientState::Connected);

        self.send_auth_message(&write).await?;
        let read = self.handle_auth_response(read, &write).await?;

        // 创建关闭信号通道
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        self.start_message_loop_if_available(read, write, shutdown_rx)
            .await;

        Ok(())
    }

    /// 构建 WebSocket URL
    fn build_websocket_url(&self, room_id: &str) -> String {
        let host = &self.config.server_host;
        let port = self.config.server_port;

        let protocol =
            if port == 443 || host.ends_with(".workers.dev") || host.ends_with(".dpdns.org") {
                "wss"
            } else {
                "ws"
            };

        if port == 80 || port == 443 {
            format!("{}://{}/ws?roomId={}", protocol, host, room_id)
        } else {
            format!("{}://{}:{}/ws?roomId={}", protocol, host, port, room_id)
        }
    }

    /// 带超时的 WebSocket 连接
    async fn connect_websocket_with_timeout(&self, ws_url: &str) -> Result<WsStream> {
        let connect_future = connect_async(ws_url);
        let timeout_duration = Duration::from_secs(15);

        match tokio::time::timeout(timeout_duration, connect_future).await {
            Ok(Ok((ws_stream, _))) => {
                tracing::debug!("WebSocket 连接成功");
                Ok(ws_stream)
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => {
                tracing::debug!("WebSocket 连接超时");
                Err(crate::CollaborationError::Other(format!(
                    "连接超时（{}秒）",
                    timeout_duration.as_secs()
                )))
            }
        }
    }

    /// 发送认证消息
    async fn send_auth_message(&self, write: &Arc<Mutex<WsSink>>) -> Result<()> {
        let auth_msg = ClientMessage::Auth {
            username: self.config.username.clone(),
            password: self.config.password.clone(),
        };
        let auth_json = serde_json::to_string(&auth_msg)?;

        info!("发送认证消息");
        write
            .lock()
            .await
            .send(Message::Text(auth_json.into()))
            .await?;

        self.state.set(ClientState::Authenticating);
        tracing::debug!("认证消息已发送，等待响应...");

        Ok(())
    }

    /// 启动消息循环（如果接收器可用）
    async fn start_message_loop_if_available(
        &mut self,
        read: WsStreamRead,
        write: Arc<Mutex<WsSink>>,
        shutdown_rx: mpsc::Receiver<()>,
    ) {
        let Some(message_rx) = self.message_rx.take() else {
            tracing::error!("message_rx 已经被消费，后台循环无法启动");
            return;
        };

        self.start_background_loop(read, write, message_rx, shutdown_rx)
            .await;
    }

    /// 启动后台循环（不阻塞）
    async fn start_background_loop(
        &self,
        mut read: WsStreamRead,
        write: Arc<Mutex<WsSink>>,
        mut message_rx: mpsc::UnboundedReceiver<ClientMessage>,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        let state = Arc::clone(&self.state);
        let session = Arc::clone(&self.session);
        let event_callback = self.event_callback.clone();

        tokio::spawn(async move {
            let mut heartbeat = interval(Duration::from_millis(crate::HEARTBEAT_INTERVAL_MS));

            loop {
                tokio::select! {
                    msg = read.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Err(e) = handle_server_message(
                                    &text,
                                    &state,
                                    &session,
                                    event_callback.clone()
                                ).await {
                                    error!("处理消息失败: {} - {}", e, text);
                                }
                            }
                            Some(Ok(Message::Close(_))) => {
                                state.set(ClientState::Disconnected);
                                break;
                            }
                            Some(Err(e)) => {
                                let err_str = e.to_string();
                                if err_str.contains("Tokio") && err_str.contains("shutdown") {
                                    debug!("WebSocket 连接关闭: {}", e);
                                } else {
                                    error!("WebSocket 错误: {}", e);
                                }
                                state.set(ClientState::Error);
                            }
                            None => {
                                state.set(ClientState::Disconnected);
                                break;
                            }
                            _ => {}
                        }
                    }

                    _ = heartbeat.tick() => {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        let ping = ClientMessage::Ping { timestamp };
                        if let Ok(text) = serde_json::to_string(&ping) {
                            let mut writer = write.lock().await;
                            if let Err(e) = writer.send(Message::Text(text.into())).await {
                                tracing::warn!("心跳发送失败: {}", e);
                                state.set(ClientState::Disconnected);
                                break;
                            }
                        }
                    }

                    Some(client_msg) = message_rx.recv() => {
                        if let Ok(text) = serde_json::to_string(&client_msg) {
                            debug!("WS 发送: {}", &text[..text.len().min(100)]);
                            let mut writer = write.lock().await;
                            if let Err(e) = writer.send(Message::Text(text.into())).await {
                                error!("发送消息失败: {}", e);
                                state.set(ClientState::Error);
                                break;
                            }
                            trace!("WS 消息发送完成");
                        } else {
                            error!("消息序列化失败");
                        }
                    }

                    _ = shutdown_rx.recv() => {
                        info!("收到关闭信号，后台循环退出");
                        state.set(ClientState::Disconnected);
                        break;
                    }
                }
            }
        });
    }

    /// 处理认证响应
    async fn handle_auth_response(
        &self,
        mut read: WsStreamRead,
        _write: &Arc<Mutex<WsSink>>,
    ) -> Result<WsStreamRead> {
        use tokio::time::{Duration, timeout};

        let auth_timeout = Duration::from_secs(10);

        match timeout(auth_timeout, read.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                tracing::debug!("Received auth response: {}", text);
                let msg: ServerMessage = serde_json::from_str(&text).map_err(|e| {
                    crate::CollaborationError::Other(format!(
                        "Failed to parse auth response: {} - text: {}",
                        e, text
                    ))
                })?;
                match msg {
                    ServerMessage::Authenticated {
                        user_id,
                        room,
                        users,
                        ..
                    } => {
                        let invite_code = room.invite_code.clone();
                        info!("认证成功: user_id={}, invite_code={}", user_id, invite_code);
                        self.state.set(ClientState::InRoom);

                        let room_info = RoomInfo {
                            id: room.id,
                            invite_code: room.invite_code.clone(),
                            name: room.name,
                            project_name: room.project_name.clone(),
                            project_hash: room.project_hash.clone(),
                            host_id: room.host_id.clone(),
                            user_count: room.user_count as usize,
                            max_users: room.max_users as usize,
                        };

                        {
                            let mut session = self.session.write().await;
                            session.current_user_id = Some(user_id.clone());
                            session.invite_code = Some(invite_code.clone());
                            session.current_room = Some(room_info.clone());
                        }

                        self.emit_event(CollaborationEvent::Authenticated {
                            user_id: user_id.clone(),
                            invite_code: invite_code.clone(),
                        });

                        if user_id == room.host_id {
                            info!("用户是房间 host，发送 RoomCreated 事件");
                            self.emit_event(CollaborationEvent::RoomCreated { room: room_info });
                        } else {
                            info!("用户加入现有房间，发送 RoomJoined 事件");
                            self.emit_event(CollaborationEvent::RoomJoined {
                                room: room_info,
                                users,
                            });
                        }

                        Ok(read)
                    }
                    ServerMessage::Error { error } => {
                        Err(crate::CollaborationError::Other(error))
                    }
                    _ => Err(crate::CollaborationError::Other(format!(
                        "意外的认证响应: {}",
                        text
                    ))),
                }
            }
            Ok(None) => Err(crate::CollaborationError::Other("连接已关闭".to_string())),
            Ok(Some(Err(e))) => Err(crate::CollaborationError::Other(format!(
                "WebSocket 错误: {}",
                e
            ))),
            Err(_) => Err(crate::CollaborationError::Other("认证超时".to_string())),
            _ => Err(crate::CollaborationError::Other(
                "意外的消息类型".to_string(),
            )),
        }
    }
}
