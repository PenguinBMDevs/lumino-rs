use lumino_collaboration::{ClientConfig, CollaborationClient};
use std::sync::{Arc, Mutex, MutexGuard};

/// 协作服务错误消息常量
mod messages {
    /// 默认房间名称
    pub const DEFAULT_ROOM_NAME: &str = "默认房间";
    /// 客户端未初始化
    pub const CLIENT_NOT_INITIALIZED: &str = "协作客户端未初始化";
    /// 客户端锁被污染
    pub const CLIENT_LOCK_POISONED: &str = "协作客户端锁被污染";
    /// 断开信号锁被污染
    pub const DISCONNECT_LOCK_POISONED: &str = "协作断开信号锁被污染";
}

/// 协作服务 - 处理协作连接和事件
///
/// 锁设计（2 层）：
/// - `Arc<std::sync::Mutex<Option<CollaborationClient>>>`
/// - 外层 `std::sync::Mutex` 提供同步访问
/// - `CollaborationClient` 内部使用无锁 `ClientStateCell` 与通道，跨线程调用其
///   同步方法（如 `send_mouse_position`、`is_connected`、`disconnect`）是安全的。
///
/// 同步 API 直接借出客户端调用其同步方法，不再需要 `block_in_place` 嵌套 runtime：
/// 业务消息通过 `mpsc` 通道转交后台发送循环，UI 线程调用不阻塞、不 panic。
#[derive(Clone)]
pub struct CollaborationService {
    /// 协作客户端（同步锁 + Option）
    client: Arc<Mutex<Option<CollaborationClient>>>,
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

    fn lock_client(&self) -> Result<MutexGuard<'_, Option<CollaborationClient>>, String> {
        self.client
            .lock()
            .map_err(|_| messages::CLIENT_LOCK_POISONED.to_string())
    }

    fn lock_disconnect_tx(
        &self,
    ) -> Result<MutexGuard<'_, Option<tokio::sync::oneshot::Sender<()>>>, String> {
        self.disconnect_tx
            .lock()
            .map_err(|_| messages::DISCONNECT_LOCK_POISONED.to_string())
    }

    /// 临时借出客户端执行同步操作；客户端不存在时返回未初始化错误。
    ///
    /// 闭包返回协作模块自身的 `Result<(), CollaborationError>`，此处统一转换为
    /// 服务层的 `Result<(), String>`（内层错误转为字符串），避免调用方双重 `?`。
    fn with_client<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&CollaborationClient) -> lumino_collaboration::Result<()>,
    {
        let guard = self.lock_client()?;
        match guard.as_ref() {
            Some(client) => f(client).map_err(|e| e.to_string()),
            None => Err(messages::CLIENT_NOT_INITIALIZED.to_string()),
        }
    }

    /// 连接到协作服务器（异步）
    pub async fn connect(
        &self,
        host: String,
        port: u16,
        username: String,
        room_name: Option<String>,
        invite_code: Option<String>,
    ) -> Result<(), String> {
        tracing::info!("协作: 正在连接到 {}:{} ...", host, port);

        // 异步断开已有连接
        self.disconnect_async().await;

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

        let (tx, rx) = tokio::sync::oneshot::channel();
        *self.lock_disconnect_tx()? = Some(tx);

        // 将客户端放入 Mutex 后，Spawn 后台任务取出并操作
        *self.lock_client()? = Some(client);
        let client_arc = Arc::clone(&self.client);

        tokio::spawn(async move {
            let mut rx = rx; // `&mut rx` 需要可变绑定
            // 从 Mutex 中取出客户端（作用域块确保 guard 在 .await 前被销毁）
            let mut client = match client_arc.lock() {
                Ok(mut guard) => match guard.take() {
                    Some(c) => c,
                    None => {
                        tracing::error!("协作: 客户端在连接前已被释放");
                        return;
                    }
                },
                Err(_) => {
                    tracing::error!("协作: {}", messages::CLIENT_LOCK_POISONED);
                    return;
                }
            };

            let result: Result<(), String> = if let Some(code) = invite_code {
                tracing::info!("协作: 正在加入房间 (邀请码: {})...", code);
                client
                    .join_room_and_connect(code)
                    .await
                    .map_err(|e| e.to_string())
            } else {
                let name = room_name.unwrap_or_else(|| messages::DEFAULT_ROOM_NAME.to_string());
                tracing::info!("协作: 正在创建房间: {} ...", name);
                client
                    .create_room_and_connect(name)
                    .await
                    .map_err(|e| e.to_string())
                    .map(|_| ())
            };

            match &result {
                Ok(_) => tracing::info!("协作: 连接成功!"),
                Err(e) => {
                    tracing::error!("协作: 连接失败: {}", e);
                    // 向 UI 广播连接失败事件，驱动对话框回到可重试状态
                    lumino_ui::event::emit(lumino_ui::event::Event::window(
                        lumino_ui::event::window::Event::collaboration_connect_failed(e.clone()),
                    ));
                }
            }

            // 将客户端放回（连接成功或失败后都可被后续操作访问）
            if let Ok(mut guard) = client_arc.lock() {
                *guard = Some(client);
            } else {
                tracing::error!(
                    "协作: 连接完成后无法放回客户端: {}",
                    messages::CLIENT_LOCK_POISONED
                );
            }

            tokio::select! {
                _ = &mut rx => tracing::info!("协作: 收到断开信号，后台任务退出"),
                _ = tokio::signal::ctrl_c() => tracing::info!("协作: 收到 Ctrl+C，后台任务退出"),
            }
        });

        Ok(())
    }

    /// 异步断开（供 connect 内部使用）
    async fn disconnect_async(&self) {
        if let Ok(mut guard) = self.lock_disconnect_tx()
            && let Some(tx) = guard.take()
        {
            let _ = tx.send(());
        }
        // 作用域块确保 MutexGuard（!Send）在 .await 前被销毁
        let mut client = match self.lock_client() {
            Ok(mut guard) => guard.take(),
            Err(_) => {
                tracing::error!("协作: {}", messages::CLIENT_LOCK_POISONED);
                return;
            }
        };
        if let Some(ref mut c) = client {
            let _ = c.disconnect();
        }
    }

    /// 处理协作事件
    fn handle_collaboration_event(event: lumino_collaboration::client::CollaborationEvent) {
        use lumino_collaboration::client::CollaborationEvent;

        match event {
            CollaborationEvent::Connected => tracing::info!("协作: 已连接到服务器"),
            CollaborationEvent::Authenticated {
                user_id,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 认证成功! 用户ID: {}, 邀请码: {}",
                    user_id,
                    invite_code
                );
                lumino_ui::event::emit(lumino_ui::event::Event::window(
                    lumino_ui::event::window::Event::collaboration_authenticated(
                        user_id,
                        invite_code,
                    ),
                ));
            }
            CollaborationEvent::RoomCreated { room } => {
                tracing::info!("协作: 房间创建成功! 邀请码: {}", room.invite_code);
                lumino_ui::event::emit(lumino_ui::event::Event::window(
                    lumino_ui::event::window::Event::collaboration_room_created(
                        room.name,
                        room.invite_code,
                    ),
                ));
            }
            CollaborationEvent::RoomJoined { room, users } => {
                tracing::info!(
                    "协作: 加入房间成功! 房间: {}, 用户数: {}",
                    room.name,
                    users.len()
                );
                lumino_ui::event::emit(lumino_ui::event::Event::window(
                    lumino_ui::event::window::Event::collaboration_room_joined(
                        room.name,
                        room.invite_code,
                        users.len(),
                    ),
                ));
            }
            CollaborationEvent::Disconnected => {
                tracing::info!("协作: 连接断开");
                lumino_ui::event::emit(lumino_ui::event::Event::window(
                    lumino_ui::event::window::Event::collaboration_disconnected(),
                ));
            }
            CollaborationEvent::UserLeft { user_id } => {
                lumino_ui::event::emit(lumino_ui::event::Event::window(
                    lumino_ui::event::window::Event::collaboration_user_left(user_id),
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
                lumino_ui::event::emit(lumino_ui::event::Event::window(
                    lumino_ui::event::window::Event::collaboration_mouse_update(
                        user_id, position.x, position.y, color, username,
                    ),
                ));
            }
            CollaborationEvent::NoteBatch { user_id, operation } => {
                if let Ok(json) = serde_json::to_string(&operation) {
                    lumino_ui::event::emit(lumino_ui::event::Event::window(
                        lumino_ui::event::window::Event::collaboration_note_update(user_id, json),
                    ));
                }
            }
            CollaborationEvent::ProjectUpdate { user_id, update } => {
                if let Ok(json) = serde_json::to_string(&update) {
                    lumino_ui::event::emit(lumino_ui::event::Event::window(
                        lumino_ui::event::window::Event::collaboration_project_update(
                            user_id, json,
                        ),
                    ));
                }
            }
            CollaborationEvent::Error { message } => tracing::error!("协作错误: {}", message),
            _ => {}
        }
    }

    /// 发送鼠标位置（同步 API）
    pub fn send_mouse_position(
        &self,
        position: lumino_collaboration::types::MousePosition,
    ) -> Result<(), String> {
        self.with_client(|client| client.send_mouse_position(position))
    }

    /// 断开连接（同步 API）
    pub fn disconnect(&self) -> Result<(), String> {
        if let Ok(mut guard) = self.lock_disconnect_tx()
            && let Some(tx) = guard.take()
        {
            let _ = tx.send(());
        }
        let mut guard = self.lock_client()?;
        let mut client = guard.take();
        drop(guard);

        if let Some(ref mut c) = client {
            let _ = c.disconnect();
        }
        // 连接已终止，不放回客户端
        Ok(())
    }

    /// 发送音符批量操作（同步 API）
    pub fn send_note_batch(
        &self,
        operation: lumino_collaboration::types::NoteBatchOperation,
    ) -> Result<(), String> {
        self.with_client(|client| client.send_note_batch(operation))
    }

    /// 发送工程更新（同步 API）
    pub fn send_project_update(
        &self,
        update: lumino_collaboration::types::ProjectUpdate,
    ) -> Result<(), String> {
        self.with_client(|client| client.send_project_update(update))
    }

    /// 检查客户端是否已连接（同步 API，真值语义）
    ///
    /// 委托给 `CollaborationClient::is_connected()`，返回真实连接状态而非仅判断
    /// 客户端对象是否存在。
    pub fn is_connected(&self) -> bool {
        match self.lock_client() {
            Ok(guard) => guard.as_ref().is_some_and(|client| client.is_connected()),
            Err(_) => false,
        }
    }
}

impl Default for CollaborationService {
    fn default() -> Self {
        Self::new()
    }
}
