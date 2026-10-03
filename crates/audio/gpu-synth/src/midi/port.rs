//! MIDI 端口 → 全局通道映射原语（REND-002 #87）。
//!
//! 语义与 `lumino-midi-model::multi_port` 完全一致（GPU 链路不引入该依赖，
//! 这里是等价实现）：单端口（port=0）恒等；产品上限 16 端口 = 256 全局通道；
//! 超出上限的端口折叠到端口 15 块（B1 决策，告警由导出入口负责）。

/// 每个 MIDI 端口占用的通道数（标准 MIDI 通道数）。
pub const CHANNELS_PER_PORT: u8 = 16;

/// 产品支持的端口数上限（REND-002 决策 B1）：16 端口 = 256 全局通道。
pub const MAX_PORTS: u8 = 16;

/// 将 u7 端口号（0..=127）钳制到产品上限内（0..=15）。
///
/// 超出上限的端口统一折叠到端口 15 块——同一文件内 port 16..127 的事件会与
/// 端口 15 共享通道状态，这是有意的降级行为（B1 决策）。
#[inline]
pub fn effective_port(port: u8) -> u8 {
    port.min(MAX_PORTS - 1)
}

/// `(port, channel)` → 全局通道号（0..=255，u8 恰好容纳 16×16）。
///
/// `channel` 仅取低 4 位（MIDI 通道语义 0..15），与
/// `OutputConnection` 的 `ch & MIDI_CHANNEL_MASK` 截断语义一致。
#[inline]
pub fn global_channel(port: u8, channel: u8) -> u8 {
    effective_port(port) * CHANNELS_PER_PORT + (channel & 0x0F)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_zero_is_identity() {
        for ch in 0u8..16 {
            assert_eq!(global_channel(0, ch), ch, "port=0 时通道应恒等");
        }
    }

    #[test]
    fn multi_port_maps_to_port_blocks() {
        assert_eq!(global_channel(1, 0), 16);
        assert_eq!(global_channel(1, 9), 25, "端口 1 的 ch9 → 25");
        assert_eq!(global_channel(6, 15), 111, "Night Voyager 7 端口边界");
        assert_eq!(global_channel(15, 15), 255, "16 端口上限");
    }

    #[test]
    fn over_limit_port_folds_to_last_block() {
        assert_eq!(global_channel(16, 0), 240, "port16 折叠到 15 块");
        assert_eq!(global_channel(127, 15), 255, "u7 边界同样折叠");
    }

    #[test]
    fn channel_uses_low_nibble_only() {
        assert_eq!(global_channel(1, 0x1F), global_channel(1, 15));
        assert_eq!(global_channel(0, 0xFF), 15);
    }
}
