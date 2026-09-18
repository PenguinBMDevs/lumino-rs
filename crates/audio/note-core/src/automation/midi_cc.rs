//! 自动化 lane → MIDI CC 序列展开（RPN / NRPN）
//!
//! MIDI 规范中 RPN/NRPN 没有独立的“事件类型”，必须展开为 CC 序列：
//! - 选择参数：RPN 用 CC101(MSB)/CC100(LSB)，NRPN 用 CC99(MSB)/CC98(LSB)；
//! - 写入数据：Data Entry CC6(MSB)/CC38(LSB)。
//!
//! 本模块把 [`AutomationLane`] 的 RPN/NRPN 事件展开为可直接下发的
//! `(tick, controller, value)` 序列（不含 channel，由调用方附加）。
//!
//! ## 位宽策略（统一 14-bit 字节对，避免歧义）
//!
//! 每个采样点固定发送“选择 2 条 + 数据 2 条”，共 4 条 CC：
//! - 逻辑 7-bit 目标（[`AutomationTarget::max_value`] == 127，如 RPN0/2）：
//!   `CC6 = value`、`CC38 = 0`（原值作为 14-bit 的 MSB 承载）；
//! - 逻辑 14-bit 目标（RPN1/NRPN/其他 RPN 参数）：
//!   `CC6 = value >> 7`、`CC38 = value & 0x7F`。
//!
//! 采样复用 [`AutomationLane::sample_curve`]：Step 段只输出锚点，
//! Curve 段逐 tick 输出贝塞尔插值值，并含同值合并与上限保护。

use super::{AutomationLane, AutomationTarget};

/// Data Entry MSB 控制器号。
pub const CC_DATA_ENTRY_MSB: u8 = 6;
/// Data Entry LSB 控制器号。
pub const CC_DATA_ENTRY_LSB: u8 = 38;
/// NRPN LSB 控制器号。
pub const CC_NRPN_LSB: u8 = 98;
/// NRPN MSB 控制器号。
pub const CC_NRPN_MSB: u8 = 99;
/// RPN LSB 控制器号。
pub const CC_RPN_LSB: u8 = 100;
/// RPN MSB 控制器号。
pub const CC_RPN_MSB: u8 = 101;

impl AutomationLane {
    /// 将 RPN/NRPN lane 展开为 MIDI CC 序列 `(tick, controller, value)`。
    ///
    /// 仅处理 [`AutomationTarget::Rpn`] / [`AutomationTarget::Nrpn`]；
    /// CC 目标（单条 CC 直发）与 PitchBend（非 CC）返回空序列。
    ///
    /// 每个采样点先发参数选择（RPN: CC101→CC100；NRPN: CC99→CC98），
    /// 再发数据字节（CC6→CC38），保证同 tick 内“选择先于数据”。
    pub fn to_midi_cc_events(&self, max_events: usize) -> Vec<(u32, u8, u8)> {
        // 参数选择字节对（MSB, LSB）；非 RPN/NRPN 目标无 CC 序列。
        let select: [(u8, u8); 2] = match &self.target {
            AutomationTarget::Rpn { parameter } => [
                (CC_RPN_MSB, (*parameter >> 7) as u8),
                (CC_RPN_LSB, (*parameter & 0x7F) as u8),
            ],
            AutomationTarget::Nrpn { parameter } => [
                (CC_NRPN_MSB, (*parameter >> 7) as u8),
                (CC_NRPN_LSB, (*parameter & 0x7F) as u8),
            ],
            AutomationTarget::CC { .. } | AutomationTarget::PitchBend => return Vec::new(),
        };
        // 逻辑 7-bit 目标（RPN0/2）按“MSB 承载原值、LSB=0”对齐到 14-bit 字节对；
        // 其余目标按标准 14-bit 拆分。
        let seven_bit = self.target.max_value() == 127;

        let samples = self.sample_curve(max_events);
        let mut out: Vec<(u32, u8, u8)> = Vec::with_capacity(samples.len().saturating_mul(4));
        for (tick, value) in samples {
            out.push((tick, select[0].0, select[0].1));
            out.push((tick, select[1].0, select[1].1));
            let (msb, lsb) = if seven_bit {
                (value.min(127) as u8, 0)
            } else {
                let v = value.min(16_383);
                ((v >> 7) as u8, (v & 0x7F) as u8)
            };
            out.push((tick, CC_DATA_ENTRY_MSB, msb));
            out.push((tick, CC_DATA_ENTRY_LSB, lsb));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{AutomationEvent, SegmentShape};

    /// 构造测试 lane（展开结果不含 channel，固定为 0）。
    fn lane(target: AutomationTarget, events: Vec<AutomationEvent>) -> AutomationLane {
        AutomationLane {
            target,
            track: 0,
            channel: 0,
            events,
        }
    }

    fn step(tick: u32, value: u16) -> AutomationEvent {
        AutomationEvent::new(tick, value, SegmentShape::Step)
    }

    /// RPN0（逻辑 7-bit）统一按 14-bit 字节对发送：MSB 承载原值，LSB 恒为 0。
    #[test]
    fn rpn0_uniform_two_byte_encoding() {
        let l = lane(AutomationTarget::Rpn { parameter: 0 }, vec![step(0, 12)]);
        assert_eq!(
            l.to_midi_cc_events(10_000),
            vec![(0, 101, 0), (0, 100, 0), (0, 6, 12), (0, 38, 0)]
        );
    }

    /// RPN1（微调，14-bit）：8500 = MSB 66 / LSB 52；8192 中心 = 64/0。
    #[test]
    fn rpn1_14bit_split_bytes() {
        let l = lane(
            AutomationTarget::Rpn { parameter: 1 },
            vec![step(0, 8_500), step(240, 8_192)],
        );
        assert_eq!(
            l.to_midi_cc_events(10_000),
            vec![
                (0, 101, 0),
                (0, 100, 1),
                (0, 6, 66),
                (0, 38, 52),
                (240, 101, 0),
                (240, 100, 1),
                (240, 6, 64),
                (240, 38, 0),
            ]
        );
    }

    /// RPN2（粗调，逻辑 7-bit）：MSB 承载原值，LSB 恒为 0。
    #[test]
    fn rpn2_coarse_two_byte_encoding() {
        let l = lane(AutomationTarget::Rpn { parameter: 2 }, vec![step(0, 64)]);
        assert_eq!(
            l.to_midi_cc_events(10_000),
            vec![(0, 101, 0), (0, 100, 2), (0, 6, 64), (0, 38, 0)]
        );
    }

    /// RPN 参数地址为 14-bit：0x0123 → CC101=2 / CC100=35。
    #[test]
    fn rpn_parameter_address_two_bytes() {
        let l = lane(
            AutomationTarget::Rpn { parameter: 0x0123 },
            vec![step(0, 0x0123)],
        );
        assert_eq!(
            l.to_midi_cc_events(10_000),
            vec![(0, 101, 2), (0, 100, 35), (0, 6, 2), (0, 38, 35)]
        );
    }

    /// NRPN 使用 CC99/CC98 选择，数据固定 14-bit 双字节。
    #[test]
    fn nrpn_uses_cc99_cc98() {
        let l = lane(
            AutomationTarget::Nrpn { parameter: 99 },
            vec![step(0, 16_383)],
        );
        assert_eq!(
            l.to_midi_cc_events(10_000),
            vec![(0, 99, 0), (0, 98, 99), (0, 6, 127), (0, 38, 127)]
        );
    }

    /// 每个采样点内必须是“选择 → 数据”顺序。
    #[test]
    fn selection_precedes_data_at_each_sample() {
        let l = lane(
            AutomationTarget::Rpn { parameter: 1 },
            vec![step(0, 0), step(480, 16_383)],
        );
        let out = l.to_midi_cc_events(10_000);
        assert_eq!(out.len(), 8, "2 个采样点 × 4 条 CC");
        assert_eq!(
            out.iter().map(|&(_, cc, _)| cc).collect::<Vec<_>>(),
            vec![101, 100, 6, 38, 101, 100, 6, 38]
        );
    }

    /// Step 段不产生中间事件：只在锚点输出。
    #[test]
    fn step_lane_emits_anchor_points_only() {
        let l = lane(
            AutomationTarget::Rpn { parameter: 1 },
            vec![step(0, 0), step(4, 64)],
        );
        let ticks: Vec<u32> = l
            .to_midi_cc_events(10_000)
            .iter()
            .map(|&(t, _, _)| t)
            .collect();
        assert_eq!(ticks, vec![0, 0, 0, 0, 4, 4, 4, 4]);
    }

    /// Curve 段逐 tick 采样（与 PitchBend 实时路径同语义）。
    #[test]
    fn curve_lane_samples_each_tick() {
        let mut l = lane(
            AutomationTarget::Rpn { parameter: 1 },
            vec![
                AutomationEvent::new(0, 0, SegmentShape::Curve { tension: 0 }),
                AutomationEvent::new(4, 127, SegmentShape::Curve { tension: 0 }),
            ],
        );
        l.recompute_auto_handles();
        let out = l.to_midi_cc_events(10_000);
        // tick 0..=4 共 5 个采样点 × 4 条 CC
        assert_eq!(out.len(), 20);
        let first_tick = out.first().map(|&(t, _, _)| t).expect("非空");
        let last_tick = out.last().map(|&(t, _, _)| t).expect("非空");
        assert_eq!((first_tick, last_tick), (0, 4));
        // 最后一个点：127 = MSB 0 / LSB 127
        assert_eq!(
            &out[16..],
            &[(4, 101, 0), (4, 100, 1), (4, 6, 0), (4, 38, 127)]
        );
    }

    /// 非 RPN/NRPN 目标返回空序列（由各自既有路径处理）。
    #[test]
    fn other_targets_are_empty() {
        let cc = lane(AutomationTarget::CC { controller: 7 }, vec![step(0, 100)]);
        assert!(cc.to_midi_cc_events(10_000).is_empty());
        let pb = lane(AutomationTarget::PitchBend, vec![step(0, 8_192)]);
        assert!(pb.to_midi_cc_events(10_000).is_empty());
    }

    #[test]
    fn empty_lane_is_empty() {
        let l = lane(AutomationTarget::Rpn { parameter: 1 }, Vec::new());
        assert!(l.to_midi_cc_events(10_000).is_empty());
    }
}
