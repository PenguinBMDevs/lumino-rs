//! 协作客户端
//!
//! 新架构：HTTP API + WebSocket (通过 RoomDurableObject)

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::time::interval;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::protocol::Message,
};
use tracing::{debug, error, info};

use crate::http::{CreateRoomResponse, HttpClient};
use crate::types::*;

/// 类型别名
type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = futures::stream::SplitSink<WsStream, Message>;
type WsStreamRead = futures::stream::SplitStream<WsStream>;

pub mod event;
pub mod handlers;
pub mod message;
pub mod state;

pub use event::{CollaborationEvent, EventCallback};
pub use handlers::handle_server_message;
pub use message::{ClientMessage, ServerMessage};
pub use state::{ClientState, CollaborationSession};

/// 协作客户端
pub struct CollaborationClient {
    config: ClientConfig,
    state: Arc<RwLock<ClientState>>,
    session: Arc<RwLock<CollaborationSession>>,
    message_tx: mpsc::UnboundedSender<ClientMessage>,
    message_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<ClientMessage>>>>,
    event_callback: Option<EventCallback>,
    shutdown_tx: Option<mpsc::Sender<()>>,
    http_client: HttpClient,
    room_id: Option<String>,
}

impl CollaborationClient {
    /// 创建新客户端
    pub fn new(config: ClientConfig) -> Self {
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let http_client = HttpClient::new(&config.server_host, config.server_port);

        Self {
            config,
            state: Arc::new(RwLock::new(ClientState::Disconnected)),
            session: Arc::new(RwLock::new(CollaborationSession::default())),
            message_tx,
            message_rx: Arc::new(Mutex::new(Some(message_rx))),
            event_callback: None,
            shutdown_tx: None,
            http_client,
            room_id: None,
        }
    }

    /// 设置事件回调
    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(CollaborationEvent) + Send + Sync + 'static,
    {
        self.event_callback = Some(Arc::new(callback));
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

    /// 连接到服务器（遗留兼容接口，请使用 create_room_and_connect 或 join_room_and_connect）
    pub async fn connect(
        &mut self,
        _host: Option<String>,
        _port: Option<u16>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Err("请使用 create_room_and_connect 或 join_room_and_connect".into())
    }

    /// 创建房间（遗留兼容接口，请使用 create_room_and_connect）
    pub async fn create_room(&self, _name: String) -> Result<(), Box<dyn std::error::Error>> {
        Err("请使用 create_room_and_connect".into())
    }

    /// 加入房间（遗留兼容接口，请使用 join_room_and_connect）
    pub async fn join_room(&self, _invite_code: String) -> Result<(), Box<dyn std::error::Error>> {
        Err("请使用 join_room_and_connect".into())
    }

    /// 创建房间并连接
    /// 新流程：HTTP 创建房间 -> WebSocket 连接（带 roomId）
    pub async fn create_room_and_connect(
        &mut self,
        room_name: String,
    ) -> Result<CreateRoomResponse, Box<dyn std::error::Error>> {
        // 步骤1：HTTP 创建房间
        info!("通过 HTTP 创建房间: {}", room_name);
        let create_response = self
            .http_client
            .create_room(&room_name, &self.generate_user_id())
            .await?;

        info!(
            "房间创建成功: id={}, invite_code={}",
            create_response.room.id, create_response.room.invite_code
        );

        // 保存房间信息
        self.room_id = Some(create_response.room.invite_code.clone());
        {
            let mut session = self.session.write().await;
            session.invite_code = Some(create_response.room.invite_code.clone());
            session.current_room = Some(RoomInfo {
                id: create_response.room.id.clone(),
                invite_code: create_response.room.invite_code.clone(),
                name: create_response.room.name.clone(),
                host_id: create_response.room.host_id.clone(),
                user_count: 1,
                max_users: 10,
            });
        }

        // 步骤2：使用 roomId 连接 WebSocket
        info!("使用 roomId 连接 WebSocket");
        self.connect_with_room_id(&create_response.room.invite_code)
            .await?;

        Ok(create_response)
    }

    /// 加入房间并连接
    pub async fn join_room_and_connect(
        &mut self,
        invite_code: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("准备加入房间: {}", invite_code);

        // 保存房间信息
        self.room_id = Some(invite_code.clone());
        {
            let mut session = self.session.write().await;
            session.invite_code = Some(invite_code.clone());
        }

        // 使用 roomId 连接 WebSocket
        tracing::debug!("Client B 准备调用 connect_with_room_id");
        self.connect_with_room_id(&invite_code).await?;
        tracing::debug!("Client B connect_with_room_id 完成");

        Ok(())
    }

    /// 使用 roomId 连接 WebSocket
    async fn connect_with_room_id(
        &mut self,
        room_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let host = &self.config.server_host;
        let port = self.config.server_port;

        // 构建 WebSocket URL（带 roomId 参数）
        let protocol =
            if port == 443 || host.ends_with(".workers.dev") || host.ends_with(".dpdns.org") {
                "wss"
            } else {
                "ws"
            };

        let ws_url = if port == 80 || port == 443 {
            format!("{}://{}/ws?roomId={}", protocol, host, room_id)
        } else {
            format!("{}://{}:{}/ws?roomId={}", protocol, host, port, room_id)
        };

        tracing::debug!("连接到 WebSocket: {}", ws_url);
        *self.state.write().await = ClientState::Connecting;

        // 建立 WebSocket 连接
        let connect_future = connect_async(&ws_url);
        let timeout_duration = Duration::from_secs(15);

        let (ws_stream, _) = match tokio::time::timeout(timeout_duration, connect_future).await {
            Ok(result) => {
                tracing::debug!("WebSocket 连接成功");
                result?
            }
            Err(_) => {
                tracing::debug!("WebSocket 连接超时");
                return Err(format!("连接超时（{}秒）", timeout_duration.as_secs()).into());
            }
        };

        info!("WebSocket 连接成功");
        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));
        *self.state.write().await = ClientState::Connected;

        // 发送认证消息
        let auth_msg = ClientMessage::Auth {
            username: self.config.username.clone(),
        };
        let auth_json = serde_json::to_string(&auth_msg)?;
        info!("发送认证消息");
        write
            .lock()
            .await
            .send(Message::Text(auth_json.into()))
            .await?;
        *self.state.write().await = ClientState::Authenticating;
        tracing::debug!("认证消息已发送，等待响应...");

        // 等待认证响应
        info!("等待认证响应...");
        self.handle_auth_response(&mut read, &write).await?;

        // 启动消息处理循环（在后台运行，不阻塞）
        self.start_background_loop(read, write).await;

        Ok(())
    }

    /// 启动后台循环（不阻塞）
    /// 注意：必须在异步上下文中调用
    async fn start_background_loop(&self, mut read: WsStreamRead, write: Arc<Mutex<WsSink>>) {
        let state = self.state.clone();
        let session = self.session.clone();
        let event_callback = self.event_callback.clone();

        // 获取 message_rx，如果已经被消费则记录错误并返回
        let mut message_rx = match self.message_rx.lock().await.take() {
            Some(rx) => rx,
            None => {
                error!("message_rx 已经被消费，后台循环无法启动");
                return;
            }
        };

        // 启动后台任务
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
                                *state.write().await = ClientState::Disconnected;
                                break;
                            }
                            Some(Err(e)) => {
                                let err_str = e.to_string();
                                if err_str.contains("Tokio") && err_str.contains("shutdown") {
                                    debug!("WebSocket 连接关闭: {}", e);
                                } else {
                                    error!("WebSocket 错误: {}", e);
                                }
                                *state.write().await = ClientState::Error;
                            }
                            None => {
                                *state.write().await = ClientState::Disconnected;
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
                            let mut w = write.lock().await;
                            let _ = w.send(Message::Text(text.into())).await;
                        }
                    }

                    Some(client_msg) = message_rx.recv() => {
                        if let Ok(text) = serde_json::to_string(&client_msg) {
                            debug!("WS 发送: {}", &text[..text.len().min(100)]);
                            let mut w = write.lock().await;
                            if let Err(e) = w.send(Message::Text(text.into())).await {
                                error!("发送消息失败: {}", e);
                                *state.write().await = ClientState::Error;
                                break;
                            }
                            info!("WS 消息发送完成");
                        } else {
                            error!("消息序列化失败");
                        }
                    }
                }
            }
        });
    }

    /// 处理认证响应
    async fn handle_auth_response(
        &self,
        read: &mut WsStreamRead,
        _write: &Arc<Mutex<WsSink>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::time::{Duration, timeout};

        let auth_timeout = Duration::from_secs(10);

        match timeout(auth_timeout, read.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                tracing::debug!("Received auth response: {}", text);
                let msg: ServerMessage = serde_json::from_str(&text).map_err(|e| {
                    format!("Failed to parse auth response: {} - text: {}", e, text)
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
                        *self.state.write().await = ClientState::InRoom;

                        // 构建 RoomInfo
                        let room_info = crate::types::RoomInfo {
                            id: room.id,
                            invite_code: room.invite_code.clone(),
                            name: room.name,
                            host_id: room.host_id.clone(),
                            user_count: room.user_count as usize,
                            max_users: room.max_users as usize,
                        };

                        // 保存房间信息
                        {
                            let mut session = self.session.write().await;
                            session.current_user_id = Some(user_id.clone());
                            session.invite_code = Some(invite_code.clone());
                            session.current_room = Some(room_info.clone());
                        }

                        // 发射认证事件
                        self.emit_event(CollaborationEvent::Authenticated {
                            user_id: user_id.clone(),
                            invite_code: invite_code.clone(),
                        });

                        // 发射房间事件（根据是否是 host 决定类型）
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

                        Ok(())
                    }
                    ServerMessage::Error { error } => Err(format!("认证失败: {}", error).into()),
                    _ => Err("意外的认证响应".into()),
                }
            }
            Ok(None) => Err("连接已关闭".into()),
            Ok(Some(Err(e))) => Err(format!("WebSocket 错误: {}", e).into()),
            Err(_) => Err("认证超时".into()),
            _ => Err("意外的消息类型".into()),
        }
    }

    /// 发送鼠标位置
    pub async fn send_mouse_position(
        &self,
        position: MousePosition,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
    pub async fn send_note_batch(
        &self,
        operation: NoteBatchOperation,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::NoteBatch { notes: operation })
            .await
    }

    /// 发送 MIDI 事件
    pub async fn send_midi_event(
        &self,
        event: MidiEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::MidiEvent { event }).await
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

    // 内部方法
    async fn send_message(&self, msg: ClientMessage) -> Result<(), Box<dyn std::error::Error>> {
        let state = *self.state.read().await;
        if !matches!(
            state,
            ClientState::Connected | ClientState::Authenticated | ClientState::InRoom
        ) {
            return Err(format!("客户端未连接，当前状态: {:?}", state).into());
        }

        self.message_tx.send(msg).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn emit_event(&self, event: CollaborationEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }

    fn generate_user_id(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("user_{}_{}", timestamp, rand::random::<u32>())
    }
}

// 添加随机数生成
mod rand {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    pub fn random<T: Default>() -> T {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        // hash 值计算完成，用于随机化默认值
        let _hash = hasher.finish();
        T::default()
    }
}
