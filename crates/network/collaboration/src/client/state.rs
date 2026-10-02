use std::sync::atomic::{AtomicU8, Ordering};

use crate::types::{InviteCode, RemoteUser, RoomInfo, UserId};

/// 客户端状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    /// 已断开连接
    Disconnected,
    /// 正在连接
    Connecting,
    /// 已连接
    Connected,
    /// 正在认证
    Authenticating,
    /// 认证完成
    Authenticated,
    /// 已加入房间
    InRoom,
    /// 出现错误
    Error,
}

impl ClientState {
    /// 将状态编码为 `u8`，用于无锁原子存储
    const fn as_u8(self) -> u8 {
        match self {
            ClientState::Disconnected => 0,
            ClientState::Connecting => 1,
            ClientState::Connected => 2,
            ClientState::Authenticating => 3,
            ClientState::Authenticated => 4,
            ClientState::InRoom => 5,
            ClientState::Error => 6,
        }
    }

    /// 从 `u8` 解码状态，未知值回退为 `Disconnected` 以保证健壮性
    const fn from_u8(value: u8) -> Self {
        match value {
            0 => ClientState::Disconnected,
            1 => ClientState::Connecting,
            2 => ClientState::Connected,
            3 => ClientState::Authenticating,
            4 => ClientState::Authenticated,
            5 => ClientState::InRoom,
            6 => ClientState::Error,
            _ => ClientState::Disconnected,
        }
    }

    /// 是否处于“已连接”活动态（可收发业务消息）
    pub(crate) const fn is_active(self) -> bool {
        matches!(
            self,
            ClientState::Connected | ClientState::Authenticated | ClientState::InRoom
        )
    }
}

/// 无锁客户端状态单元
///
/// 使用 `AtomicU8` 承载 [`ClientState`] 编码值，避免在热路径（鼠标位置同步、心跳、
/// 事件处理）上频繁争用 `RwLock`。读写为 `Relaxed` 序：状态本身是离散枚举，丢失中间态
/// 不会破坏不变量，且调用方不会基于该值做跨变量的临界区判断。
#[derive(Debug, Default)]
pub struct ClientStateCell {
    /// 存储 `ClientState::as_u8()` 的原子值
    value: AtomicU8,
}

impl ClientStateCell {
    /// 创建初始为 `Disconnected` 的状态单元
    pub fn new() -> Self {
        Self {
            value: AtomicU8::new(ClientState::Disconnected.as_u8()),
        }
    }

    /// 读取当前状态
    pub fn get(&self) -> ClientState {
        ClientState::from_u8(self.value.load(Ordering::Relaxed))
    }

    /// 覆盖写入状态
    pub fn set(&self, state: ClientState) {
        self.value.store(state.as_u8(), Ordering::Relaxed);
    }

    /// 仅当当前处于“活动态”时才更新为 `next`，用于连接中断时避免被陈旧事件覆盖
    ///
    /// 这里手写 CAS 循环，而不用 `AtomicU8::fetch_update`：后者自 Rust 1.99 起被弃用，
    /// 替代品 `try_update` 需要 `atomic_try_update` feature（rust-lang/rust#135894），
    /// 在本 crate 声明的 MSRV 1.92.0 上仍是 unstable（E0658）——直接改名会让 MSRV 失效。
    /// `compare_exchange_weak` 自 1.0 起稳定，语义与 `fetch_update` 的内部循环完全一致：
    /// 仅在观测到的当前值处于活动态时写入，否则放弃写入。
    pub fn set_if_active(&self, next: ClientState) {
        let next_u8 = next.as_u8();
        let mut current = self.value.load(Ordering::Relaxed);
        loop {
            if !ClientState::from_u8(current).is_active() {
                return;
            }
            match self.value.compare_exchange_weak(
                current,
                next_u8,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                // 竞争失败（或 weak 伪失败）：以最新观测值重试判定
                Err(actual) => current = actual,
            }
        }
    }

    /// 是否处于活动态
    pub fn is_active(&self) -> bool {
        ClientState::from_u8(self.value.load(Ordering::Relaxed)).is_active()
    }
}

/// 协作会话信息
#[derive(Debug, Clone, Default)]
pub struct CollaborationSession {
    /// 当前用户 ID
    pub current_user_id: Option<UserId>,
    /// 当前房间邀请码
    pub invite_code: Option<InviteCode>,
    /// 当前所在房间信息
    pub current_room: Option<RoomInfo>,
    /// 远程在线用户映射
    pub remote_users: std::collections::HashMap<UserId, RemoteUser>,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// 覆盖写入后应能读回同一状态
    #[test]
    fn test_client_state_cell_set_then_get() {
        let cell = ClientStateCell::new();
        assert_eq!(cell.get(), ClientState::Disconnected, "初始应为断连态");
        cell.set(ClientState::InRoom);
        assert_eq!(cell.get(), ClientState::InRoom);
        assert!(cell.is_active(), "InRoom 属于活动态");
    }

    /// 非活动态下 `set_if_active` 必须放弃写入（陈旧事件不得复活会话）
    #[test]
    fn test_set_if_active_skips_inactive() {
        let cell = ClientStateCell::new();
        cell.set_if_active(ClientState::InRoom);
        assert_eq!(
            cell.get(),
            ClientState::Disconnected,
            "Disconnected 为非活动态，不应被覆盖"
        );

        cell.set(ClientState::Error);
        cell.set_if_active(ClientState::Connected);
        assert_eq!(cell.get(), ClientState::Error, "Error 同样属于非活动态");
    }

    /// 活动态下 `set_if_active` 应写入目标状态
    #[test]
    fn test_set_if_active_updates_active() {
        for from in [
            ClientState::Connected,
            ClientState::Authenticated,
            ClientState::InRoom,
        ] {
            let cell = ClientStateCell::new();
            cell.set(from);
            cell.set_if_active(ClientState::Error);
            assert_eq!(
                cell.get(),
                ClientState::Error,
                "{from:?} 为活动态，应允许写入"
            );
        }
    }

    /// CAS 循环在并发下不产生非法编码，且活动态不变量始终成立
    #[test]
    fn test_set_if_active_is_atomic_under_contention() {
        let targets = [
            ClientState::Connected,
            ClientState::Authenticated,
            ClientState::InRoom,
        ];
        let cell = Arc::new(ClientStateCell::new());
        cell.set(ClientState::Connected);

        std::thread::scope(|scope| {
            let handles: Vec<_> = targets
                .iter()
                .map(|&target| {
                    let cell = Arc::clone(&cell);
                    scope.spawn(move || {
                        for _ in 0..1_000 {
                            cell.set_if_active(target);
                        }
                    })
                })
                .collect();
            for handle in handles {
                handle.join().expect("并发写入线程不应 panic");
            }
        });

        let final_state = cell.get();
        assert!(
            targets.contains(&final_state),
            "终态应为某个写入目标：{final_state:?}"
        );
        assert!(cell.is_active(), "终态应保持活动态：{final_state:?}");
    }
}
