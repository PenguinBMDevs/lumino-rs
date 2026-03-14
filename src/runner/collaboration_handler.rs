use std::sync::Arc;
use tokio::sync::Mutex;

/// 协作状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollaborationStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
}

/// 协作处理器
#[derive(Clone)]
pub struct CollaborationHandler {
    client: Option<Arc<Mutex<lumino_collaboration::CollaborationClient>>>,
    status: CollaborationStatus,
}

impl CollaborationHandler {
    pub fn new() -> Self {
        Self {
            client: None,
            status: CollaborationStatus::default(),
        }
    }

    /// 连接到协作服务器
    pub async fn connect(
        &mut self,
        host: String,
        port: u16,
        username: String,
    ) -> Result<(), String> {
        use lumino_collaboration::client::CollaborationEvent;

        // 更新状态为连接中
        self.status = CollaborationStatus::Connecting;

        // 创建协作客户端配置
        let config = lumino_collaboration::ClientConfig {
            server_host: host.clone(),
            server_port: port,
            username: username.clone(),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
        };

        // 创建协作客户端
        let mut client = lumino_collaboration::CollaborationClient::new(config);

        // 设置事件回调
        client.set_event_callback(move |event| {
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
        });

        // 使用 Arc<Mutex<>> 包装客户端以便在异步任务中共享
        let client = Arc::new(Mutex::new(client));
        let client_clone = client.clone();

        // 保存客户端
        self.client = Some(client);

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

        tracing::info!("协作: 正在连接到 {}:{} ...", host, port);
        Ok(())
    }

    /// 创建房间
    pub async fn create_room(&self, name: String) -> Result<(), String> {
        tracing::info!("协作: 请求创建房间 - {}", name);
        if let Some(client) = &self.client {
            let client = client.clone();
            tokio::spawn(async move {
                let c = client.lock().await;
                if let Err(e) = c.create_room(name) {
                    tracing::error!("协作: 创建房间失败: {}", e);
                }
            });
            Ok(())
        } else {
            Err("协作客户端未初始化".to_string())
        }
    }

    /// 加入房间
    pub async fn join_room(&self, invite_code: String) -> Result<(), String> {
        tracing::info!("协作: 请求加入房间 - {}", invite_code);
        if let Some(client) = &self.client {
            let client = client.clone();
            tokio::spawn(async move {
                let c = client.lock().await;
                if let Err(e) = c.join_room(invite_code) {
                    tracing::error!("协作: 加入房间失败: {}", e);
                }
            });
            Ok(())
        } else {
            Err("协作客户端未初始化".to_string())
        }
    }

    /// 断开连接
    pub async fn disconnect(&mut self) -> Result<(), String> {
        tracing::info!("协作: 请求断开连接");
        if let Some(client) = self.client.take() {
            tokio::spawn(async move {
                let mut c = client.lock().await;
                if let Err(e) = c.disconnect().await {
                    tracing::error!("协作: 断开连接失败: {}", e);
                }
            });
        }
        self.status = CollaborationStatus::Disconnected;
        Ok(())
    }

    /// 获取当前状态
    pub fn status(&self) -> CollaborationStatus {
        self.status
    }
}
