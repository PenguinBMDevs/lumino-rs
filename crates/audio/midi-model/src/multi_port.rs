//! 多端口（MIDI Port）→ 全局通道映射原语（REND-002 CPU 侧基础层）。
//!
//! 多端口 MIDI 文件（FF 21 MidiPort meta）里，每个端口的 16 个 MIDI 通道
//! 相互独立。合成层（xsynth `SynthFormat::Custom { channels }`）只认全局通道号，
//! 因此统一映射为 `global = port * 16 + channel`。
//!
//! 单端口文档（port 恒为 0）映射恒等（`global == channel`）、通道数 16，
//! 与既有行为完全一致——这是多端口落地的零行为变化基线。
//!
//! Phase 1（解锁）提供本模块原语与端口上限；导出/实时/追齐的**接线**
//! 在后续阶段进行，接线前所有调用方走 port=0 恒等路径。

/// 每个 MIDI 端口占用的通道数（标准 MIDI 通道数）。
pub const CHANNELS_PER_PORT: u16 = 16;

/// 产品支持的端口数上限（REND-002 决策 B1）：16 端口 = 256 全局通道。
///
/// FF 21 的端口是 u7（0..=127），但合成层不做 2048 通道预分配；超出本上限
/// 的端口由 [`effective_port`] 折叠到端口 15 块，调用方必须显式告警并计数
/// （口径：不静默丢音、不整体拒绝）。
pub const MAX_PORTS: u8 = 16;

/// `(port, channel)` → 全局通道号。
///
/// `channel` 仅取低 4 位（MIDI 通道语义 0..16）；`port` 为 0..=127（FF 21 为 u7）。
/// 返回 `u16`：16 端口 × 16 通道 = 256 已超出 u8 表达范围，统一用 u16 防溢出。
#[inline]
pub fn global_channel(port: u8, channel: u8) -> u16 {
    u16::from(port) * CHANNELS_PER_PORT + u16::from(channel & 0x0F)
}

/// 全局通道号 → `(port, channel)`（[`global_channel`] 的逆映射）。
#[inline]
pub fn split_global_channel(global: u16) -> (u8, u8) {
    debug_assert!(
        u32::from(global).div_ceil(u32::from(CHANNELS_PER_PORT)) <= u32::from(u8::MAX),
        "全局通道号超出端口 u8 表达范围: {global}"
    );
    (
        (global / CHANNELS_PER_PORT) as u8,
        (global % CHANNELS_PER_PORT) as u8,
    )
}

/// 覆盖端口 `0..=max_port` 所需的总通道数（`(max_port + 1) * 16`）。
///
/// 例：单端口文档 → 16；Night Voyager（端口 0..=6）→ 112。
///
/// 注意：调用方应先对 `max_port` 取 [`effective_port`] 再传入，
/// 保证不超出 [`MAX_PORTS`] 对应的 256 通道上限。
#[inline]
pub fn channels_for_max_port(max_port: u8) -> u32 {
    (u32::from(max_port) + 1) * u32::from(CHANNELS_PER_PORT)
}

/// 将 u7 端口号（0..=127）钳制到产品上限范围内（0..=[`MAX_PORTS`]-1）。
///
/// 超出上限的端口统一折叠到端口 15 块——同一文件内 port 16..127 的事件会与
/// 端口 15 共享通道状态，这是有意的降级行为（B1 决策）。调用方**必须在折叠
/// 发生时显式告警并计数**，避免静默丢音或静默串台。
#[inline]
pub fn effective_port(port: u8) -> u8 {
    port.min(MAX_PORTS - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_zero_is_identity() {
        // 零行为变化基线：单端口文档（port=0）映射恒等。
        for ch in 0u8..16 {
            assert_eq!(global_channel(0, ch), u16::from(ch), "port=0 时通道应恒等");
        }
    }

    #[test]
    fn multi_port_maps_to_port_blocks() {
        assert_eq!(global_channel(1, 0), 16);
        assert_eq!(global_channel(1, 9), 25, "端口 1 的 ch9（打击乐）→ 25");
        assert_eq!(global_channel(1, 15), 31);
        // Night Voyager 实测素材端口 0..=6。
        assert_eq!(global_channel(6, 0), 96);
        assert_eq!(global_channel(6, 15), 111);
    }

    #[test]
    fn channel_uses_low_nibble_only() {
        // 与 OutputConnection 的 `ch & MIDI_CHANNEL_MASK` 截断语义一致。
        assert_eq!(global_channel(1, 0x1F), global_channel(1, 15));
        assert_eq!(global_channel(0, 0xFF), 15);
    }

    #[test]
    fn u7_port_boundary_fits_u16() {
        // FF 21 端口为 u7（0..=127）：127 * 16 + 15 = 2047，仍在 u16 内。
        assert_eq!(global_channel(127, 15), 2047);
    }

    #[test]
    fn split_roundtrips_global_channel() {
        for port in [0u8, 1, 6, 15, 127] {
            for ch in [0u8, 9, 15] {
                let g = global_channel(port, ch);
                assert_eq!(split_global_channel(g), (port, ch), "g={g} 往返失败");
            }
        }
    }

    #[test]
    fn channels_for_max_port_counts_blocks_of_16() {
        assert_eq!(channels_for_max_port(0), 16, "单端口 → 16 通道（现状）");
        assert_eq!(channels_for_max_port(1), 32);
        assert_eq!(channels_for_max_port(6), 112, "7 端口素材 → 112");
        assert_eq!(channels_for_max_port(15), 256, "16 端口 → 256（已超 u8）");
        assert_eq!(channels_for_max_port(127), 2048, "u7 边界");
    }

    #[test]
    fn effective_port_clamps_beyond_product_limit() {
        // 上限内恒等。
        for port in 0u8..MAX_PORTS {
            assert_eq!(effective_port(port), port, "端口 {port} 不应被折叠");
        }
        // 超限统一折叠到端口 15 块（B1 决策），不丢事件、不整体拒绝。
        for port in MAX_PORTS..=u8::MAX {
            assert_eq!(effective_port(port), MAX_PORTS - 1, "端口 {port} 应折叠");
        }
        assert_eq!(
            channels_for_max_port(effective_port(127)),
            256,
            "上限 256 通道"
        );
    }
}
