use lumino_collaboration::{ClientConfig, CollaborationClient};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

/// 协作服务错误消息常量
mod messages {
    /// 默认房间名称
    pub const DEFAULT_ROOM_NAME: &str = "默认房间";
    /// Mutex 污染错误消息
    pub const MUTEX_POISONED: &str = "协作服务: client Mutex 已被污染，可能有线程 panic";
    /// 无法获取客户端锁
    pub const LOCK_FAILED: &str = "协作服务: 无法获取客户端锁";
    /// 客户端未初始化
    pub const CLIENT_NOT_INITIALIZED: &str = "协作客户端未初始化";
    /// 遗留接口提示 - 创建房间
    pub const USE_CONNECT_CREATE_ROOM: &str = "请使用 connect 方法创建房间";
    /// 遗留接口提示 - 加入房间
    pub const USE_CONNECT_JOIN_ROOM: &str = "请使用 connect 方法加入房间";
}

/// 协作服务 - 处理协作连接和事件
///
/// 该服务负责处理与协作服务器的连接、房间管理和事件处理。
/// 包括连接认证、房间创建/加入、鼠标同步和音符同步等功能。
#[derive(Clone)]
pub struct CollaborationService {
    client: Arc<Mutex<Option<Arc<TokioMutex<CollaborationClient>>>>>,
}

impl CollaborationService {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// 安全地获取客户端锁
    ///
    /// 当 Mutex 被 poison 时，记录错误并返回 None
    fn lock_client(
        &self,
    ) -> Option<std::sync::MutexGuard<'_, Option<Arc<TokioMutex<CollaborationClient>>>>> {
        match self.client.lock() {
            Ok(guard) => Some(guard),
            Err(e) => {
                tracing::error!("{}: {}", messages::MUTEX_POISONED, e);
                None
            }
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

        // 创建协作客户端配置
        let config = ClientConfig {
            server_host: host.clone(),
            server_port: port,
            username: username.clone(),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
        };

        // 创建协作客户端
        let mut client = CollaborationClient::new(config);

        // 设置事件回调
        client.set_event_callback(move |event| {
            Self::handle_collaboration_event(event);
        });

        // 使用 Arc<Mutex<>> 包装客户端以便在异步任务中共享
        let client = Arc::new(TokioMutex::new(client));
        let client_clone = client.clone();

        // 保存客户端
        {
            let Some(mut guard) = self.lock_client() else {
                return Err(messages::LOCK_FAILED.to_string());
            };
            *guard = Some(client);
        }

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
            // 连接完成后释放锁，让其他操作（如 send_mouse_position）可以获取
            // client_clone 通过 Arc 保持引用，后台循环在 CollaborationClient 内部运行
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
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
                // 通知 UI 切换到创建/加入房间界面
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationAuthenticated {
                        user_id,
                        invite_code,
                    },
                ));
            }
            CollaborationEvent::RoomCreated { room } => {
                tracing::info!("协作: 房间创建成功! 邀请码: {}", room.invite_code);
                // 通知 UI 切换到房间内界面
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
                // 通知 UI 切换到房间内界面
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
                // 通知 UI 重置状态
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
                // 通知 UI 更新远端游标
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
                // 通知 UI 更新远端音符
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
        let Some(client_guard) = self.lock_client() else {
            return Err(messages::LOCK_FAILED.to_string());
        };

        if let Some(client) = client_guard.clone() {
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

    /// 创建房间（遗留兼容接口，实际未使用——所有调用都走 connect）
    #[allow(dead_code)]
    pub fn create_room(&self, _name: String) -> Result<(), String> {
        Err(messages::USE_CONNECT_CREATE_ROOM.to_string())
    }

    /// 加入房间（遗留兼容接口，实际未使用——所有调用都走 connect）
    #[allow(dead_code)]
    pub fn join_room(&self, _invite_code: String) -> Result<(), String> {
        Err(messages::USE_CONNECT_JOIN_ROOM.to_string())
    }

    /// 断开连接
    pub fn disconnect(&self) -> Result<(), String> {
        let Some(client_guard) = self.lock_client() else {
            return Err(messages::LOCK_FAILED.to_string());
        };

        if let Some(client) = client_guard.clone() {
            tokio::spawn(async move {
                let mut c = client.lock().await;
                if let Err(e) = c.disconnect().await {
                    tracing::error!("协作: 断开连接失败: {}", e);
                }
            });
            Ok(())
        } else {
            Err(messages::CLIENT_NOT_INITIALIZED.to_string())
        }
    }

    /// 发送音符批量操作
    pub fn send_note_batch(
        &self,
        operation: lumino_collaboration::types::NoteBatchOperation,
    ) -> Result<(), String> {
        let Some(client_guard) = self.lock_client() else {
            return Err(messages::LOCK_FAILED.to_string());
        };

        if let Some(client) = client_guard.clone() {
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

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        let Some(client_guard) = self.lock_client() else {
            tracing::warn!("协作服务: is_connected 无法获取锁，返回 false");
            return false;
        };

        let is_connected = client_guard.is_some();
        tracing::debug!("协作服务 is_connected: {}", is_connected);
        is_connected
    }
}
