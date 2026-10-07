use lumino_collaboration::{ClientConfig, CollaborationClient};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// 协作服务错误消息常量
mod messages {
    /// 默认房间名称
    pub const DEFAULT_ROOM_NAME: &str = "默认房间";
    /// 客户端未初始化
    pub const CLIENT_NOT_INITIALIZED: &str = "协作客户端未初始化";
    /// 客户端锁被污染
    pub const CLIENT_LOCK_POISONED: &str = "协作客户端锁被污染";
}

/// 客户端槽位（DEBT-06 #123：显式建模连接生命周期）。
///
/// `token` 为每次 connect 递增的代数：连接完成时只有槽位仍属于该 token
/// （`Connecting { token }`）才允许提交，否则直接丢弃——晚到的旧连接不得
/// 覆盖新连接，也不得在用户断开后"复活"。
pub(crate) enum ClientSlot<C> {
    /// 未连接
    Idle,
    /// 连接中（握手未完成）；token 标识本次连接
    Connecting {
        /// 本次连接代数
        token: u64,
    },
    /// 已连接
    Connected(C),
}

impl<C> ClientSlot<C> {
    /// 槽位是否仍是该 token 的"连接中"状态（提交守卫）。
    fn is_current_connecting(&self, token: u64) -> bool {
        matches!(self, ClientSlot::Connecting { token: t } if *t == token)
    }

    /// 已连接时对内部客户端执行判定（服务层复用，避免辅助方法成为死代码）。
    fn is_connected_with(&self, check: impl FnOnce(&C) -> bool) -> bool {
        match self {
            ClientSlot::Connected(c) => check(c),
            _ => false,
        }
    }

    /// 取出已连接客户端并把槽位归零（连接中/空闲返回 None）。
    fn take_client(&mut self) -> Option<C> {
        match std::mem::replace(self, ClientSlot::Idle) {
            ClientSlot::Connected(c) => Some(c),
            _ => None,
        }
    }
}

/// 协作服务 - 处理协作连接和事件
///
/// 锁设计（2 层）：
/// - `Mutex<ClientSlot<CollaborationClient>>`：槽位状态机（Idle/Connecting/Connected）
/// - `CollaborationClient` 内部使用无锁 `ClientStateCell` 与通道，跨线程调用其
///   同步方法（如 `send_mouse_position`、`is_connected`、`disconnect`）是安全的。
///
/// 同步 API 直接借出客户端调用其同步方法，不再需要 `block_in_place` 嵌套 runtime：
/// 业务消息通过 `mpsc` 通道转交后台发送循环，UI 线程调用不阻塞、不 panic。
#[derive(Clone)]
pub struct CollaborationService {
    /// 客户端槽位状态机
    slot: Arc<Mutex<ClientSlot<CollaborationClient>>>,
    /// 连接代数计数器（每次 connect/disconnect 递增）
    next_token: Arc<AtomicU64>,
    /// 已提交连接的后台包装任务停止信号
    disconnect_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl CollaborationService {
    pub fn new() -> Self {
        Self {
            slot: Arc::new(Mutex::new(ClientSlot::Idle)),
            next_token: Arc::new(AtomicU64::new(0)),
            disconnect_tx: Arc::new(Mutex::new(None)),
        }
    }

    fn lock_slot(&self) -> Result<MutexGuard<'_, ClientSlot<CollaborationClient>>, String> {
        self.slot
            .lock()
            .map_err(|_| messages::CLIENT_LOCK_POISONED.to_string())
    }

    /// 使任何在途 connect 失效，并取回当前已连接客户端。
    ///
    /// 返回 `Some(client)` 表示取回了一个"已提交"的连接（调用方负责
    /// `client.disconnect()`）；`None` 表示只有在途连接或本就空闲。
    fn take_current_client(&self) -> Result<Option<CollaborationClient>, String> {
        // 递增代数：Connecting 的提交守卫立即失效，晚到的结果会被丢弃。
        self.next_token.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = self.disconnect_tx.lock()
            && let Some(tx) = guard.take()
        {
            let _ = tx.send(());
        }
        let mut slot = self.lock_slot()?;
        Ok(slot.take_client())
    }

    /// 临时借出客户端执行同步操作；客户端不存在时返回未初始化错误。
    ///
    /// 闭包返回协作模块自身的 `Result<(), CollaborationError>`，此处统一转换为
    /// 服务层的 `Result<(), String>`（内层错误转为字符串），避免调用方双重 `?`。
    fn with_client<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&CollaborationClient) -> lumino_collaboration::Result<()>,
    {
        let guard = self.lock_slot()?;
        match &*guard {
            ClientSlot::Connected(client) => f(client).map_err(|e| e.to_string()),
            _ => Err(messages::CLIENT_NOT_INITIALIZED.to_string()),
        }
    }

    /// 连接到协作服务器（异步）
    ///
    /// DEBT-06 #123：并发 connect 由槽位代数仲裁——只有最后一次 connect 的
    /// 结果会提交；旧的晚到结果直接丢弃，`disconnect` 后不可能"复活"。
    pub async fn connect(
        &self,
        host: String,
        port: u16,
        username: String,
        password: String,
        room_name: Option<String>,
        invite_code: Option<String>,
    ) -> Result<(), String> {
        tracing::info!("协作: 正在连接到 {}:{} ...", host, port);

        // 异步断开已有连接（同时使在途连接失效）
        self.disconnect_async().await;

        let token = self.next_token.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut slot = self.lock_slot()?;
            *slot = ClientSlot::Connecting { token };
        }

        let config = ClientConfig {
            server_host: host.clone(),
            server_port: port,
            username: username.clone(),
            password,
        };

        let slot_arc = Arc::clone(&self.slot);
        let stop_tx_slot = Arc::clone(&self.disconnect_tx);

        tokio::spawn(async move {
            let mut client = CollaborationClient::new(config);
            client.set_event_callback(move |event| {
                Self::handle_collaboration_event(event);
            });

            let result: Result<(), String> = if let Some(code) = invite_code {
                tracing::info!("协作: 正在加入房间 (邀请码: {})...", code);
                client
                    .join_room_and_connect(code)
                    .await
                    .map_err(|e| e.to_string())
                    .map(|_| ())
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
                Ok(()) => {
                    tracing::info!("协作: 连接成功!");
                    let committed = match slot_arc.lock() {
                        Ok(mut slot) => {
                            if slot.is_current_connecting(token) {
                                *slot = ClientSlot::Connected(client);
                                true
                            } else {
                                false
                            }
                        }
                        Err(_) => {
                            tracing::error!("协作: {}", messages::CLIENT_LOCK_POISONED);
                            false
                        }
                    };
                    if !committed {
                        // 连接结果已过期（被更新的 connect/disconnect 取代）：
                        // 直接丢弃旧客户端，绝不覆盖当前状态。
                        tracing::info!("协作: 连接结果已过期（已被新的连接/断开取代），丢弃");
                        return;
                    }
                    // 提交成功：登记停止信号，包装任务等待断开指令后退出。
                    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
                    if let Ok(mut guard) = stop_tx_slot.lock() {
                        *guard = Some(stop_tx);
                    }
                    let _ = stop_rx.await;
                }
                Err(e) => {
                    tracing::error!("协作: 连接失败: {}", e);
                    // 只有仍是当前连接时才广播失败并归零；过期失败静默丢弃。
                    let current = match slot_arc.lock() {
                        Ok(mut slot) => {
                            if slot.is_current_connecting(token) {
                                *slot = ClientSlot::Idle;
                                true
                            } else {
                                false
                            }
                        }
                        Err(_) => false,
                    };
                    if current {
                        lumino_ui::event::emit(lumino_ui::event::Event::window(
                            lumino_ui::event::window::Event::collaboration_connect_failed(e),
                        ));
                    }
                }
            }
        });

        Ok(())
    }

    /// 异步断开（供 connect 内部使用）
    async fn disconnect_async(&self) {
        match self.take_current_client() {
            Ok(Some(mut client)) => {
                let _ = client.disconnect();
            }
            Ok(None) => {}
            Err(e) => tracing::error!("协作: {}", e),
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
                        room.project_name.clone(),
                        room.project_hash.clone(),
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
                        room.project_name.clone(),
                        room.project_hash.clone(),
                    ),
                ));
            }
            CollaborationEvent::Selection { user_id, selection } => {
                tracing::debug!("协作: 收到远端选择更新 - 用户: {}", user_id);
                lumino_ui::event::emit(lumino_ui::event::Event::window(
                    lumino_ui::event::window::Event::collaboration_selection(
                        user_id,
                        selection.to_string(),
                        String::new(),
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
            CollaborationEvent::Error { message } => {
                tracing::error!("协作错误: {}", message);
                // 踢人 / 房间解散等错误必须弹出独立对话框提示，不能只写日志。
                // 在独立线程弹出原生对话框，避免阻塞协作网络线程。
                let _msg = message.clone();
                std::thread::spawn(move || {
                    #[cfg(not(test))]
                    {
                        let _ = rfd::MessageDialog::new()
                            .set_title("协作通知")
                            .set_description(&_msg)
                            .set_level(rfd::MessageLevel::Warning)
                            .show();
                    }
                });
            }
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
    ///
    /// DEBT-06 #123：使在途 connect 失效（代数递增 + 槽位归零）；已提交的连接
    /// 取回并断开。重复调用幂等，断开后 `is_connected()` 不会"复活"。
    pub fn disconnect(&self) -> Result<(), String> {
        if let Some(mut client) = self.take_current_client()? {
            let _ = client.disconnect();
        }
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

    /// 发送本地选择变更（同步 API）
    pub fn send_selection(&self, selection: serde_json::Value) -> Result<(), String> {
        self.with_client(|client| client.send_selection(selection))
    }

    /// 检查客户端是否已连接（同步 API，真值语义）
    ///
    /// 委托给 `CollaborationClient::is_connected()`，返回真实连接状态而非仅判断
    /// 槽位是否存在。
    pub fn is_connected(&self) -> bool {
        match self.lock_slot() {
            Ok(slot) => slot.is_connected_with(|client| client.is_connected()),
            Err(_) => false,
        }
    }
}

impl Default for CollaborationService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DEBT-06 #123：并发 connect 只有"当前 token"允许提交，旧结果必须丢弃。
    #[test]
    fn stale_connect_result_is_rejected() {
        // connect#1(token=1) 之后又发起 connect#2(token=2)：槽位属于 2。
        let mut slot: ClientSlot<u32> = ClientSlot::Connecting { token: 2 };
        assert!(
            !slot.is_current_connecting(1),
            "旧连接的完成结果不得覆盖新连接"
        );
        assert!(slot.is_current_connecting(2), "当前连接允许提交");

        // connect#2 提交后，更旧的 token 也不得再提交（防止复活/覆盖）。
        slot = ClientSlot::Connected(42);
        assert!(!slot.is_current_connecting(2));
    }

    /// 断开使在途连接失效：槽位回 Idle，晚到的提交被静默丢弃。
    #[test]
    fn disconnect_invalidates_inflight_connect() {
        let mut slot: ClientSlot<u32> = ClientSlot::Connecting { token: 7 };
        assert!(slot.take_client().is_none(), "连接中无客户端可取");
        assert!(!slot.is_current_connecting(7), "断开后 token 立即失效");
        assert!(!slot.is_connected_with(|_| true));
    }

    /// 断开取回已连接客户端并归零；重复断开幂等（不复活）。
    #[test]
    fn disconnect_takes_connected_client_once() {
        let mut slot: ClientSlot<u32> = ClientSlot::Connected(9);
        assert_eq!(slot.take_client(), Some(9));
        assert!(!slot.is_connected_with(|_| true));
        assert_eq!(slot.take_client(), None, "重复断开应为空操作");
    }
}
