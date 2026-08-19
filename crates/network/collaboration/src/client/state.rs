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
    pub fn set_if_active(&self, next: ClientState) {
        let next_u8 = next.as_u8();
        self.value
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                if ClientState::from_u8(current).is_active() {
                    Some(next_u8)
                } else {
                    None
                }
            })
            .ok();
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
