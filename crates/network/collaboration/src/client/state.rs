use std::sync::atomic::{AtomicU8, Ordering};

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

    /// 是否处于活动态
    pub fn is_active(&self) -> bool {
        ClientState::from_u8(self.value.load(Ordering::Relaxed)).is_active()
    }
}

#[cfg(test)]
mod tests {
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
}
