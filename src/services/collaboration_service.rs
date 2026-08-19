use lumino_collaboration::{ClientConfig, CollaborationClient};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::task::block_in_place;

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
/// - `CollaborationClient` 内部已使用 `Arc<RwLock<>>`，无需额外异步锁
///
/// 同步 API 桥接异步方法时，临时取出 Client 再放回。
///
/// # Runtime 嵌套处理
///
/// 主函数使用 `#[tokio::main]`，winit 事件循环在 tokio runtime 内运行。
/// 同步方法（如 `send_mouse_position`）通过 `block_in_place` +
/// `Handle::block_on()` 安全调用异步方法，避免 nested runtime panic。
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

            match result {
                Ok(_) => tracing::info!("协作: 连接成功!"),
                Err(e) => tracing::error!("协作: 连接失败: {}", e),
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
            let _ = c.disconnect().await;
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

    /// 在同步上下文中临时取出客户端，调用异步操作后再放回。
    ///
    /// 使用 `tokio::task::block_in_place` + `Handle::block_on()` 驱动异步调用。
    /// `block_in_place` 会临时释放当前线程的 async 上下文（`ENTERED` 标记），
    /// 允许 `Handle::block_on()` 安全地 re-enter runtime，避免 nested runtime panic。
    ///
    /// # Safety
    ///
    /// 当前函数被 `#[tokio::main]` async main 调用（winit 事件循环在 tokio runtime 内），
    /// 不能直接使用 `Handle::current().block_on()` —— 它会在 `enter()` 时因当前线程
    /// 已持有 `ENTERED` 标记而 panic。`block_in_place` 是解决此问题的标准 tokio 模式。
    fn with_client_async<F>(&self, f: F) -> Result<(), String>
    where
        F: for<'a> FnOnce(
            &'a CollaborationClient,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = lumino_collaboration::Result<()>> + 'a>,
        >,
    {
        let mut guard = self.lock_client()?;
        let client = guard.take();
        drop(guard);

        let client_result = match client {
            Some(ref c) => block_in_place(move || {
                let handle = tokio::runtime::Handle::current();
                handle.block_on(f(c)).map_err(|e| e.to_string())
            }),
            None => Err(messages::CLIENT_NOT_INITIALIZED.to_string()),
        };

        if let Ok(mut guard) = self.lock_client() {
            *guard = client;
        } else {
            tracing::error!(
                "协作: 异步调用后无法放回客户端: {}",
                messages::CLIENT_LOCK_POISONED
            );
        }
        client_result
    }

    /// 发送鼠标位置（同步 API）
    pub fn send_mouse_position(
        &self,
        position: lumino_collaboration::types::MousePosition,
    ) -> Result<(), String> {
        self.with_client_async(|client| Box::pin(client.send_mouse_position(position)))
    }

    /// 断开连接（同步 API）
    ///
    /// 同样使用 `block_in_place` 处理 tokio runtime 嵌套问题。
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
            block_in_place(move || {
                let handle = tokio::runtime::Handle::current();
                let _ = handle.block_on(c.disconnect());
            });
        }
        // 连接已终止，不放回客户端
        Ok(())
    }

    /// 发送音符批量操作（同步 API）
    pub fn send_note_batch(
        &self,
        operation: lumino_collaboration::types::NoteBatchOperation,
    ) -> Result<(), String> {
        self.with_client_async(|client| Box::pin(client.send_note_batch(operation)))
    }

    /// 发送工程更新（同步 API）
    pub fn send_project_update(
        &self,
        update: lumino_collaboration::types::ProjectUpdate,
    ) -> Result<(), String> {
        self.with_client_async(|client| Box::pin(client.send_project_update(update)))
    }

    /// 检查客户端实例是否存在（同步 API）
    ///
    /// 纯同步操作，无需 `block_in_place`。
    pub fn is_connected(&self) -> bool {
        self.lock_client().is_ok_and(|guard| guard.is_some())
    }
}

impl Default for CollaborationService {
    fn default() -> Self {
        Self::new()
    }
}
