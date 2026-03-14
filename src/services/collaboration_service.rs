use lumino_collaboration::{ClientConfig, CollaborationClient};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

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

    /// 连接到协作服务器
    pub async fn connect(&self, host: String, port: u16, username: String) -> Result<(), String> {
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
            let mut guard = self.client.lock().unwrap();
            *guard = Some(client);
        }

        // 异步连接
        let host_clone = host.clone();
        let port_clone = port;
        tokio::spawn(async move {
            let mut c = client_clone.lock().await;
            match c.connect(Some(host_clone), Some(port_clone)).await {
                Ok(_) => {
                    tracing::info!("协作: 连接成功!");
                }
                Err(e) => {
                    tracing::error!("协作: 连接失败: {}", e);
                }
            }
            // 保持客户端存活
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
            CollaborationEvent::MouseUpdate {
                user_id,
                position,
                color,
            } => {
                // 通知 UI 更新远端游标
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationMouseUpdate {
                        user_id,
                        x: position.x,
                        y: position.y,
                        color,
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
}
