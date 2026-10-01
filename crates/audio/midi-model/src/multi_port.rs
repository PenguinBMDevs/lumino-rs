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

// ── 运行时打击乐模态跟踪（REND-002 方案 B）─────────────────────────

/// 单通道的打击乐模态状态。
#[derive(Debug, Clone, Copy)]
struct ChannelPercussionState {
    /// 当前是否为打击乐通道；默认 `channel % 16 == 9`（GM：每端口 ch9 为鼓）。
    percussion: bool,
    /// CC0 Bank Select MSB。
    bank_msb: u8,
    /// CC32 Bank Select LSB。
    bank_lsb: u8,
    /// 是否收到过 CC0（未收到时 CC32 不单独作为切换证据，避免误关默认 ch9）。
    msb_seen: bool,
}

/// 每通道「音符/打击乐」模态跟踪器（REND-002 方案 B，纯状态推导）。
///
/// 现实素材通过 Bank Select 约定切换鼓通道：
/// - GS：`CC0=120`（Rhythm）/`121`（SFX）；
/// - XG：`CC0=127` 且 `CC32=0`；
/// - 反向切回旋律：出现明确的非鼓 Bank Select。
///
/// xsynth 的打击乐是 `program.bank = 128` 的模态，且处于该模态时 CC0 被忽略
/// （fork `core/src/channel/params.rs:79-83`），所以调用方必须在转发触发事件
/// **之前**显式下发 `SetPercussionMode(bool)`。本类型不依赖 xsynth，导出与实时共用。
#[derive(Debug, Clone)]
pub struct PercussionTracker {
    states: Vec<ChannelPercussionState>,
}

impl PercussionTracker {
    /// 按全局通道数创建（通道 `c` 的默认模态 = `c % 16 == 9`）。
    pub fn new(channels: u32) -> Self {
        let mut states = Vec::with_capacity(channels as usize);
        for c in 0..channels {
            states.push(ChannelPercussionState {
                percussion: (c % u32::from(CHANNELS_PER_PORT)) == 9,
                bank_msb: 0,
                bank_lsb: 0,
                msb_seen: false,
            });
        }
        Self { states }
    }

    /// 观察一条 CC（仅 CC0/CC32 参与判定）。
    ///
    /// 返回 `Some(on)` 表示该通道模态需要切换为目标值，调用方必须先下发
    /// `SetPercussionMode(on)` 再转发本条 CC；`None` 表示无需动作。
    pub fn observe_cc(&mut self, channel: u16, controller: u8, value: u8) -> Option<bool> {
        let state = self.states.get_mut(channel as usize)?;
        match controller {
            0 => {
                state.bank_msb = value;
                state.msb_seen = true;
            }
            32 => {
                state.bank_lsb = value;
            }
            _ => return None,
        }
        if !state.msb_seen {
            // 只收到 CC32：不单独判定（常见顺序是 CC0→CC32，避免误切换默认 ch9）。
            return None;
        }
        let desired = drum_bank_evidence(state.bank_msb, state.bank_lsb);
        if desired != state.percussion {
            state.percussion = desired;
            Some(desired)
        } else {
            None
        }
    }

    /// 指定通道当前是否为打击乐。
    #[inline]
    pub fn is_percussion(&self, channel: u16) -> bool {
        self.states
            .get(channel as usize)
            .is_some_and(|s| s.percussion)
    }

    /// 是否收到过 Bank Select 证据（追齐重发只针对有证据的通道）。
    #[inline]
    pub fn has_evidence(&self, channel: u16) -> bool {
        self.states
            .get(channel as usize)
            .is_some_and(|s| s.msb_seen)
    }

    /// 通道数（全局通道空间）。
    #[inline]
    pub fn channels(&self) -> u32 {
        self.states.len() as u32
    }
}

/// Bank Select → 是否为鼓通道证据（GS/XG 约定）。
#[inline]
fn drum_bank_evidence(bank_msb: u8, bank_lsb: u8) -> bool {
    match bank_msb {
        120 | 121 => true,    // GS Rhythm / SFX
        127 => bank_lsb == 0, // XG Drum
        _ => false,
    }
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

    #[test]
    fn percussion_tracker_default_is_per_port_ch9() {
        let tracker = PercussionTracker::new(32);
        assert!(tracker.is_percussion(9), "端口 0 ch9 默认打击乐");
        assert!(!tracker.is_percussion(0), "端口 0 ch0 默认旋律");
        assert!(tracker.is_percussion(25), "端口 1 ch9（25）默认打击乐");
        assert!(!tracker.is_percussion(24), "端口 1 ch8（24）默认旋律");
    }

    #[test]
    fn percussion_tracker_gs_rhythm_switches_both_ways() {
        let mut tracker = PercussionTracker::new(16);
        assert_eq!(tracker.observe_cc(3, 0, 120), Some(true), "GS Rhythm → 鼓");
        assert!(tracker.is_percussion(3));
        assert_eq!(tracker.observe_cc(3, 0, 8), Some(false), "非鼓 bank → 旋律");
        assert!(!tracker.is_percussion(3));
    }

    #[test]
    fn percussion_tracker_xg_requires_lsb_zero() {
        let mut tracker = PercussionTracker::new(16);
        assert_eq!(tracker.observe_cc(2, 32, 5), None, "未收到 CC0 不单独判定");
        assert_eq!(
            tracker.observe_cc(2, 0, 127),
            None,
            "127+LSB5 非鼓，且默认本非鼓 → 无需切换"
        );
        assert_eq!(tracker.observe_cc(2, 32, 0), Some(true), "127+LSB0 → 鼓");
        assert_eq!(
            tracker.observe_cc(2, 0, 0),
            Some(false),
            "回归普通 bank → 旋律"
        );
    }

    #[test]
    fn percussion_tracker_can_demote_default_ch9() {
        let mut tracker = PercussionTracker::new(16);
        assert_eq!(
            tracker.observe_cc(9, 0, 0),
            Some(false),
            "ch9 收到明确非鼓 bank → 转旋律"
        );
        assert_eq!(tracker.observe_cc(9, 0, 120), Some(true), "再切回鼓");
    }

    #[test]
    fn percussion_tracker_evidence_only_counts_cc0() {
        let mut tracker = PercussionTracker::new(16);
        assert!(!tracker.has_evidence(0));
        tracker.observe_cc(0, 32, 1);
        assert!(!tracker.has_evidence(0), "仅 CC32 不算证据");
        tracker.observe_cc(0, 0, 1);
        assert!(tracker.has_evidence(0), "CC0 才算证据");
    }
}
