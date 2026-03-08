use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::interval;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info};

use crate::types::*;
use crate::{HEARTBEAT_INTERVAL_MS};

/// 客户端到服务器的消息
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum ClientMessage {
    Auth { username: String },
    CreateRoom { name: String },
    JoinRoom { #[serde(rename = "inviteCode")] invite_code: String },
    LeaveRoom,
    MouseMove { position: MousePosition },
    NoteBatch { notes: NoteBatchOperation },
    MidiEvent { event: MidiEvent },
    MidiEventBatch { events: Vec<MidiEvent> },
    ProjectUpdate { update: ProjectUpdate },
    RequestSync,
    Ping { timestamp: u64 },
}

/// 服务器到客户端的消息
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum ServerMessage {
    AuthSuccess { #[serde(rename = "userId")] user_id: UserId, #[serde(rename = "inviteCode")] invite_code: InviteCode },
    AuthError { error: String },
    RoomCreated { room: RoomInfo },
    RoomJoined { room: RoomInfo, users: Vec<UserInfo>, #[serde(rename = "projectState")] project_state: serde_json::Value },
    RoomError { error: String },
    UserJoined { user: UserInfo },
    UserLeft { #[serde(rename = "userId")] user_id: UserId },
    MouseUpdate { #[serde(rename = "userId")] user_id: UserId, username: String, position: MousePosition, color: String },
    NoteBatchUpdate { #[serde(rename = "userId")] user_id: UserId, operation: NoteBatchOperation },
    MidiEventUpdate { #[serde(rename = "userId")] user_id: UserId, event: MidiEvent },
    MidiEventBatchUpdate { #[serde(rename = "userId")] user_id: UserId, events: Vec<MidiEvent> },
    ProjectStateUpdate { #[serde(rename = "userId")] user_id: UserId, update: ProjectUpdate },
    FullSync { #[serde(rename = "projectState")] project_state: serde_json::Value, users: Vec<UserInfo> },
    Pong { timestamp: u64, #[serde(rename = "serverTime")] server_time: u64 },
    Error { error: String },
}

/// 事件回调类型
pub type EventCallback = Arc<dyn Fn(CollaborationEvent) + Send + Sync>;

/// 协作事件
#[derive(Debug, Clone)]
pub enum CollaborationEvent {
    Connected,
    Disconnected,
    Authenticated { user_id: UserId, invite_code: InviteCode },
    RoomCreated { room: RoomInfo },
    RoomJoined { room: RoomInfo, users: Vec<UserInfo> },
    UserJoined { user: UserInfo },
    UserLeft { user_id: UserId },
    MouseUpdate { user_id: UserId, position: MousePosition, color: String },
    NoteBatch { user_id: UserId, operation: NoteBatchOperation },
    MidiEvent { user_id: UserId, event: MidiEvent },
    MidiEventBatch { user_id: UserId, events: Vec<MidiEvent> },
    ProjectUpdate { user_id: UserId, update: ProjectUpdate },
    FullSync { users: Vec<UserInfo> },
    Error { message: String },
}

/// 协作客户端
pub struct CollaborationClient {
    config: ClientConfig,
    state: Arc<RwLock<ClientState>>,
    session: Arc<RwLock<CollaborationSession>>,
    message_tx: mpsc::UnboundedSender<ClientMessage>,
    message_rx: Arc<Mutex<mpsc::UnboundedReceiver<ClientMessage>>>,
    event_callback: Option<EventCallback>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl CollaborationClient {
    /// 创建新客户端
    pub fn new(config: ClientConfig) -> Self {
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        Self {
            config,
            state: Arc::new(RwLock::new(ClientState::Disconnected)),
            session: Arc::new(RwLock::new(CollaborationSession::default())),
            message_tx,
            message_rx: Arc::new(Mutex::new(message_rx)),
            event_callback: None,
            shutdown_tx: None,
        }
    }

    /// 设置事件回调
    pub fn set_event_callback<F>(&mut self,
        callback: F
    ) where F: Fn(CollaborationEvent) + Send + Sync + 'static {
        self.event_callback = Some(Arc::new(callback));
    }

    /// 连接到服务器
    pub async fn connect(&mut self,
        host: Option<String>,
        port: Option<u16>
    ) -> Result<(), Box<dyn std::error::Error>> {
        let host = host.unwrap_or_else(|| self.config.server_host.clone());
        let port = port.unwrap_or(self.config.server_port);

        let ws_url = format!("ws://{}:{}/ws", host, port);
        info!("连接到服务器: {}", ws_url);

        *self.state.write().await = ClientState::Connecting;

        // 建立WebSocket连接
        let (ws_stream, _) = connect_async(&ws_url).await?;
        info!("WebSocket连接成功");

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));
        *self.state.write().await = ClientState::Connected;

        // 发送认证消息
        let auth_msg = ClientMessage::Auth {
            username: self.config.username.clone(),
        };
        let auth_json = serde_json::to_string(&auth_msg)?;
        write.lock().await.send(Message::Text(auth_json)).await?;
        *self.state.write().await = ClientState::Authenticating;

        // 等待认证响应
        info!("等待认证响应...");
        loop {
            match read.next().await {
                Some(Ok(Message::Text(text))) => {
                    info!("收到认证响应: {}", text);
                    let response: ServerMessage = match serde_json::from_str(&text) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("解析认证响应失败: {}\n响应内容: {}", e, text);
                            // 继续等待，可能下一条是认证响应
                            continue;
                        }
                    };
                    match response {
                        ServerMessage::AuthSuccess { user_id, invite_code } => {
                            info!("认证成功: user_id={}", user_id);
                            *self.state.write().await = ClientState::Authenticated;

                            let mut session = self.session.write().await;
                            session.current_user_id = Some(user_id.clone());
                            session.invite_code = Some(invite_code.clone());
                            drop(session);

                            self.emit_event(CollaborationEvent::Authenticated {
                                user_id,
                                invite_code
                            });
                            break; // 认证成功，跳出循环
                        }
                        ServerMessage::AuthError { error } => {
                            error!("认证失败: {}", error);
                            *self.state.write().await = ClientState::Error;
                            return Err(error.into());
                        }
                        other => {
                            info!("在认证阶段收到其他合法文本消息，忽略: {:?}", other);
                            continue; // 忽略并继续读取
                        }
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    error!("连接在认证前被关闭: {:?}", frame);
                    return Err("连接被关闭".into());
                }
                Some(Err(e)) => {
                    error!("WebSocket 错误: {}", e);
                    return Err(e.into());
                }
                Some(Ok(other)) => {
                    info!("收到非文本消息: {:?}", other);
                    continue; // 忽略并继续读取
                }
                None => {
                    error!("连接在认证前断开");
                    return Err("连接断开".into());
                }
            }
        }

        // 启动消息处理循环
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let state = self.state.clone();
        let session = self.session.clone();
        let message_rx = self.message_rx.clone();
        let event_callback = self.event_callback.clone();
        let write_clone = write.clone();

        tokio::spawn(async move {
            info!("消息处理循环已启动");
            let mut heartbeat = interval(Duration::from_millis(HEARTBEAT_INTERVAL_MS));

            loop {
                tokio::select! {
                    // 处理接收到的消息
                    msg = read.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                info!("收到服务器文本消息: {}", text);
                                if let Err(e) = handle_server_message(
                                    &text,
                                    &state,
                                    &session,
                                    event_callback.clone()
                                ).await {
                                    error!("处理消息失败: {} - {}", e, text);
                                    println!("处理消息失败: {} - {}", e, text); // Add debug print
                                }
                            }
                            Some(Ok(Message::Close(frame))) => {
                                info!("收到服务器关闭帧: {:?}", frame);
                                *state.write().await = ClientState::Disconnected;
                                if let Some(ref cb) = event_callback {
                                    cb(CollaborationEvent::Disconnected);
                                }
                                break;
                            }
                            Some(Ok(Message::Ping(_))) => {
                                debug!("收到服务器 ping");
                            }
                            Some(Ok(Message::Pong(_))) => {
                                debug!("收到服务器 pong");
                            }
                            Some(Ok(other)) => {
                                debug!("收到其他消息类型: {:?}", other);
                            }
                            Some(Err(e)) => {
                                error!("WebSocket错误: {}", e);
                                *state.write().await = ClientState::Error;
                                if let Some(ref cb) = event_callback {
                                    cb(CollaborationEvent::Error {
                                        message: e.to_string()
                                    });
                                }
                            }
                            None => {
                                info!("连接已关闭 (None)");
                                *state.write().await = ClientState::Disconnected;
                                if let Some(ref cb) = event_callback {
                                    cb(CollaborationEvent::Disconnected);
                                }
                                break;
                            }
                        }
                    }

                    // 发送客户端消息
                    msg = async {
                        let mut rx = message_rx.lock().await;
                        rx.recv().await
                    } => {
                        if let Some(msg) = msg {
                            let json = match serde_json::to_string(&msg) {
                                Ok(j) => j,
                                Err(e) => {
                                    error!("序列化消息失败: {}", e);
                                    continue;
                                }
                            };

                            debug!("发送消息: {}", json);
                            let mut w = write_clone.lock().await;
                            if let Err(e) = w.send(Message::Text(json)).await {
                                error!("发送消息失败: {}", e);
                            }
                        }
                    }

                    // 心跳
                    _ = heartbeat.tick() => {
                        let ping = ClientMessage::Ping {
                            timestamp: Instant::now().elapsed().as_millis() as u64
                        };
                        let json = serde_json::to_string(&ping).unwrap();
                        debug!("发送心跳: {}", json);
                        let mut w = write_clone.lock().await;
                        if let Err(e) = w.send(Message::Text(json)).await {
                            error!("发送心跳失败: {}", e);
                        }
                    }

                    // 关闭信号
                    _ = shutdown_rx.recv() => {
                        info!("收到关闭信号");
                        break;
                    }
                }
            }
            info!("消息处理循环已结束");
        });

        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        *self.state.write().await = ClientState::Disconnected;
        info!("已断开连接");

        Ok(())
    }

    /// 创建房间
    pub fn create_room(&self, name: String) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::CreateRoom { name })
    }

    /// 加入房间
    pub fn join_room(&self, invite_code: String) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::JoinRoom { invite_code })
    }

    /// 离开房间
    pub fn leave_room(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::LeaveRoom)
    }

    /// 发送鼠标位置
    pub fn send_mouse_position(
        &self,
        position: MousePosition
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::MouseMove { position })
    }

    /// 发送音符批量操作
    pub fn send_note_batch(
        &self,
        operation: NoteBatchOperation
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::NoteBatch { notes: operation })
    }

    /// 发送MIDI事件
    pub fn send_midi_event(
        &self,
        event: MidiEvent
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::MidiEvent { event })
    }

    /// 发送MIDI事件批量
    pub fn send_midi_event_batch(
        &self,
        events: Vec<MidiEvent>
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::MidiEventBatch { events })
    }

    /// 发送项目更新
    pub fn send_project_update(
        &self,
        update: ProjectUpdate
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::ProjectUpdate { update })
    }

    /// 请求同步
    pub fn request_sync(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::RequestSync)
    }

    /// 获取当前状态
    pub async fn state(&self) -> ClientState {
        *self.state.read().await
    }

    /// 获取当前会话
    pub async fn session(&self) -> CollaborationSession {
        self.session.read().await.clone()
    }

    /// 是否已连接
    pub async fn is_connected(&self) -> bool {
        matches!(
            *self.state.read().await,
            ClientState::Connected | ClientState::Authenticated | ClientState::InRoom
        )
    }

    /// 是否在房间中
    pub async fn is_in_room(&self) -> bool {
        *self.state.read().await == ClientState::InRoom
    }

    // 内部方法
    fn send_message(
        &self,
        msg: ClientMessage
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("排队发送消息: {:?}", msg);
        self.message_tx
            .send(msg)
            .map_err(|e| e.to_string().into())
    }

    fn emit_event(&self,
        event: CollaborationEvent
    ) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }
}

// 处理服务器消息
async fn handle_server_message(
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
                }
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

        ServerMessage::MouseUpdate { user_id, position, color, .. } => {
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
                    color
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
            info!("收到 RoomCreated: {:?}", room);
            let mut sess = session.write().await;
            sess.current_room = Some(room.clone());
            drop(sess);

            *state.write().await = ClientState::InRoom;

            if let Some(ref cb) = callback {
                cb(CollaborationEvent::RoomCreated { room });
            }
        }

        ServerMessage::RoomJoined { room, users, .. } => {
            info!("收到 RoomJoined: {:?}", room);
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
                        }
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
