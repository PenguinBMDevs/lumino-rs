use lumino_collaboration::{ClientConfig, CollaborationClient};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

/// 协作服务错误消息常量
mod messages {
    /// 默认房间名称
    pub const DEFAULT_ROOM_NAME: &str = "默认房间";
    /// 客户端未初始化
    pub const CLIENT_NOT_INITIALIZED: &str = "协作客户端未初始化";
}

/// 协作服务 - 处理协作连接和事件
///
/// 该服务负责处理与协作服务器的连接、房间管理和事件处理。
/// 包括连接认证、房间创建/加入、鼠标同步和音符同步等功能。
#[derive(Clone)]
pub struct CollaborationService {
    /// 协作客户端（双层包装：外层 Mutex 用于同步代码访问，
    /// 内层 Arc<TokioMutex> 用于在异步任务间共享可变所有权）
    ///
    /// 未来可简化为单层 `Arc<tokio::sync::RwLock<Option<CollaborationClient>>>`
    /// 当 CollaborationClient 的所有方法都改为 &self 后
    client: Arc<Mutex<Option<Arc<TokioMutex<CollaborationClient>>>>>,
    /// 连接断开信号（用于终止 connect 中的后台心跳循环）
    disconnect_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl CollaborationService {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
            disconnect_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// 连接到协作服务器
    ///
    /// 如果提供了 invite_code，则加入房间；否则创建新房间
    pub async fn connect(
        &self,
        host: String,
        port: u16,
        username: String,
        room_name: Option<String>,
        invite_code: Option<String>,
    ) -> Result<(), String> {
        tracing::info!("协作: 正在连接到 {}:{} ...", host, port);

        // 如果已有连接，先断开
        self.disconnect().ok();

        let config = ClientConfig {
            server_host: host.clone(),
            server_port: port,
            username: username.clone(),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
        };

        let mut client = CollaborationClient::new(config);
        client.set_event_callback(move |event| {
            Self::handle_collaboration_event(event);
        });

        let client = Arc::new(TokioMutex::new(client));
        let client_clone = client.clone();

        // 保存客户端
        *self.client.lock() = Some(client);

        // 创建断开信号通道
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        *self.disconnect_tx.lock() = Some(tx);

        // 异步连接并创建/加入房间
        tokio::spawn(async move {
            {
                let mut c = client_clone.lock().await;
                let result: Result<(), String> = if let Some(code) = invite_code {
                    tracing::info!("协作: 正在加入房间 (邀请码: {})...", code);
                    c.join_room_and_connect(code)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    let name = room_name.unwrap_or_else(|| messages::DEFAULT_ROOM_NAME.to_string());
                    tracing::info!("协作: 正在创建房间: {} ...", name);
                    c.create_room_and_connect(name)
                        .await
                        .map_err(|e| e.to_string())
                        .map(|_| ())
                };

                match result {
                    Ok(_) => {
                        tracing::info!("协作: 连接成功!");
                    }
                    Err(e) => {
                        tracing::error!("协作: 连接失败: {}", e);
                    }
                }
            }
            // 连接完成后释放锁，等待断开信号或运行时取消
            // 使用 oneshot 替代无限循环，避免永久占用 tokio 线程
            tokio::select! {
                _ = &mut rx => {
                    tracing::info!("协作: 收到断开信号，后台任务退出");
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("协作: 收到 Ctrl+C，后台任务退出");
                }
            }
        });

        Ok(())
    }

    /// 处理协作事件
    fn handle_collaboration_event(event: lumino_collaboration::client::CollaborationEvent) {
        use lumino_collaboration::client::CollaborationEvent;

        match event {
            CollaborationEvent::Connected => {
                tracing::info!("协作: 已连接到服务器");
            }
            CollaborationEvent::Authenticated {
                user_id,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 认证成功! 用户ID: {}, 邀请码: {}",
                    user_id,
                    invite_code
                );
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationAuthenticated {
                        user_id,
                        invite_code,
                    },
                ));
            }
            CollaborationEvent::RoomCreated { room } => {
                tracing::info!("协作: 房间创建成功! 邀请码: {}", room.invite_code);
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationRoomCreated {
                        room_name: room.name,
                        invite_code: room.invite_code,
                    },
                ));
            }
            CollaborationEvent::RoomJoined { room, users } => {
                tracing::info!(
                    "协作: 加入房间成功! 房间: {}, 用户数: {}",
                    room.name,
                    users.len()
                );
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationRoomJoined {
                        room_name: room.name,
                        invite_code: room.invite_code,
                        user_count: users.len(),
                    },
                ));
            }
            CollaborationEvent::Disconnected => {
                tracing::info!("协作: 连接断开");
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationDisconnected,
                ));
            }
            CollaborationEvent::UserLeft { user_id } => {
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationUserLeft { user_id },
                ));
            }
            CollaborationEvent::MouseUpdate {
                user_id,
                position,
                color,
                username,
            } => {
                tracing::debug!(
                    "协作事件 - 鼠标更新：user_id={}, x={}, y={}, color={}, username={}",
                    user_id,
                    position.x,
                    position.y,
                    color,
                    username
                );
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationMouseUpdate {
                        user_id,
                        x: position.x,
                        y: position.y,
                        color,
                        username,
                    },
                ));
            }
            CollaborationEvent::NoteBatch { user_id, operation } => {
                if let Ok(json) = serde_json::to_string(&operation) {
                    lumino_core::event::emit(lumino_core::event::Event::Window(
                        lumino_core::event::window::Event::CollaborationNoteUpdate {
                            user_id,
                            operation: json,
                        },
                    ));
                }
            }
            CollaborationEvent::Error { message } => {
                tracing::error!("协作错误: {}", message);
            }
            _ => {}
        }
    }

    /// 发送鼠标位置
    pub fn send_mouse_position(
        &self,
        position: lumino_collaboration::types::MousePosition,
    ) -> Result<(), String> {
        let client = self.client.lock().clone();

        if let Some(client) = client {
            tokio::spawn(async move {
                let c = client.lock().await;
                if let Err(e) = c.send_mouse_position(position).await {
                    tracing::debug!("协作: 发送鼠标位置失败: {}", e);
                }
            });
            Ok(())
        } else {
            Err(messages::CLIENT_NOT_INITIALIZED.to_string())
        }
    }

    /// 断开连接
    pub fn disconnect(&self) -> Result<(), String> {
        // 发送断开信号，终止后台循环
        if let Some(tx) = self.disconnect_tx.lock().take() {
            let _ = tx.send(());
        }

        let client = self.client.lock().clone();

        if let Some(client) = client {
            tokio::spawn(async move {
                let mut c = client.lock().await;
                if let Err(e) = c.disconnect().await {
                    tracing::error!("协作: 断开连接失败: {}", e);
                }
            });
        }

        *self.client.lock() = None;
        Ok(())
    }

    /// 发送音符批量操作
    pub fn send_note_batch(
        &self,
        operation: lumino_collaboration::types::NoteBatchOperation,
    ) -> Result<(), String> {
        let client = self.client.lock().clone();

        if let Some(client) = client {
            tokio::spawn(async move {
                let c = client.lock().await;
                if let Err(e) = c.send_note_batch(operation).await {
                    tracing::error!("协作: 发送音符操作失败: {}", e);
                }
            });
            Ok(())
        } else {
            Err(messages::CLIENT_NOT_INITIALIZED.to_string())
        }
    }

    /// 检查客户端实例是否存在
    ///
    /// 注意：这仅表示客户端对象已创建，不保证 WebSocket 已连接。
    /// 连接成功后 CollaborationEvent::Connected 事件会被发送。
    pub fn is_connected(&self) -> bool {
        self.client.lock().is_some()
    }
}

impl Default for CollaborationService {
    fn default() -> Self {
        Self::new()
    }
}
