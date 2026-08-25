//! Seek Chase 机制
//!
//! 跳转（Seek / 循环回绕）后，跳转点之前生效的 CC/PC/PB/RPN 控制器状态必须
//! 重放给合成器，否则音色回退默认钢琴、弯音归零、踏板丢失（yinhe 同名机制，
//! 见 `docs/issues/macOS音频引擎线程冲突分析.md` 修复方案 #6）。
//!
//! 实现：latest-wins 快照。按事件顺序扫描 seek 点之前的全部控制事件并应用到
//! [`ChannelChase`]，flush 时每通道输出一组"最终状态"消息：
//! - RPN 参数（PBS/Fine/Coarse）以完整选择序列重放（CC101→CC100→CC6→CC38），
//!   保证合成器在正确选中参数下解析 Data Entry；
//! - 其余已出现的 CC 按控制器号升序重放最新值（含 CC0/32 bank select，
//!   天然先于 ProgramChange 发出）；
//! - 最后 ProgramChange 与 PitchBend。
//!
//! 状态机为纯函数式核心（`apply` + `emit`），与事件来源解耦，便于单元测试。

use crate::playback::engine::MidiMessage;

/// RPN 参数号（仅处理 MSB=0 的三个常用参数）
const RPN_PITCH_BEND_SENSITIVITY: u8 = 0;
const RPN_FINE_TUNE: u8 = 1;
const RPN_COARSE_TUNE: u8 = 2;

/// Data Entry 写入对：MSB 必有，LSB 仅当该参数被选中期间收到过 CC38 才记录
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RpnDataEntry {
    msb: u8,
    lsb: Option<u8>,
}

/// 单 MIDI 通道的控制器状态快照（latest-wins）
#[derive(Debug, Clone)]
pub(crate) struct ChannelChase {
    /// 各控制器最新值（索引 = controller 号）
    ccs: [u8; 128],
    /// 已出现过的控制器位图（bit n = CC n 曾出现）
    cc_seen: u128,
    program: Option<u8>,
    /// 弯音值（-1.0..1.0 归一化）
    pitch_bend: Option<f32>,
    /// 当前选中的 RPN 参数号（由 CC101 MSB + CC100 LSB 组合；仅识别 MSB=0）
    rpn_selected: Option<u8>,
    rpn_msb: Option<u8>,
    rpn_lsb: Option<u8>,
    /// 三个 RPN 参数各自的 Data Entry 写入状态
    pbs: Option<RpnDataEntry>,
    fine_tune: Option<RpnDataEntry>,
    coarse_tune: Option<RpnDataEntry>,
}

impl Default for ChannelChase {
    fn default() -> Self {
        Self {
            ccs: [0; 128],
            cc_seen: 0,
            program: None,
            pitch_bend: None,
            rpn_selected: None,
            rpn_msb: None,
            rpn_lsb: None,
            pbs: None,
            fine_tune: None,
            coarse_tune: None,
        }
    }
}

impl ChannelChase {
    /// 应用一个控制事件
    ///
    /// `kind`：0=CC、1=ProgramChange、2=PitchBend（与 `PackedControlEvent.kind` 一致）；
    /// `param`：packed 参数（CC: 高 8 位 controller 低 8 位 value；
    /// PC: program；PB: 14 位原始弯音值，8192 居中）。
    pub(crate) fn apply(&mut self, kind: u8, param: u16) {
        match kind {
            0 => {
                let (cc, val) = ((param >> 8) as u8, param as u8);
                let idx = (cc & 0x7F) as usize;
                self.ccs[idx] = val;
                self.cc_seen |= 1u128 << idx;
                match cc {
                    // Data Entry 写入 → 记录到当前选中的 RPN 参数
                    6 => self.write_data_entry_msb(val),
                    38 => self.write_data_entry_lsb(val),
                    // RPN 选择：MSB=0 且 LSB 为 0/1/2 时才跟踪
                    100 => self.rpn_lsb = Some(val),
                    101 => self.rpn_msb = Some(val),
                    _ => {}
                }
                self.update_rpn_selection();
            }
            1 => self.program = Some((param & 0x7F) as u8),
            2 => {
                let raw = param as i16 - 8192;
                self.pitch_bend = Some(raw as f32 / 8192.0);
            }
            _ => {}
        }
    }

    /// 应用引擎层 `MidiMessage` 形式的控制事件（当前轨可编辑事件路径）
    pub(crate) fn apply_message(&mut self, msg: &MidiMessage) {
        match msg {
            MidiMessage::ControlChange {
                controller, value, ..
            } => self.apply(0, u16::from(*controller) << 8 | u16::from(*value)),
            MidiMessage::ProgramChange { program, .. } => self.apply(1, u16::from(*program)),
            MidiMessage::PitchBend { value, .. } => {
                let raw = ((value.clamp(-1.0, 1.0) + 1.0) * 8192.0).round() as u16;
                self.apply(2, raw);
            }
            _ => {}
        }
    }

    /// 根据 MSB/LSB 更新选中的 RPN 参数号（每次 CC 后调用，与真实合成器时序一致）
    fn update_rpn_selection(&mut self) {
        if self.rpn_msb == Some(0) {
            self.rpn_selected = match self.rpn_lsb {
                Some(l @ (RPN_PITCH_BEND_SENSITIVITY | RPN_FINE_TUNE | RPN_COARSE_TUNE)) => Some(l),
                _ => None,
            };
        } else {
            self.rpn_selected = None;
        }
    }

    fn write_data_entry_msb(&mut self, val: u8) {
        let slot = match self.rpn_selected {
            Some(RPN_PITCH_BEND_SENSITIVITY) => &mut self.pbs,
            Some(RPN_FINE_TUNE) => &mut self.fine_tune,
            Some(RPN_COARSE_TUNE) => &mut self.coarse_tune,
            _ => return,
        };
        slot.get_or_insert_with(RpnDataEntry::default).msb = val;
    }

    fn write_data_entry_lsb(&mut self, val: u8) {
        let slot = match self.rpn_selected {
            Some(RPN_PITCH_BEND_SENSITIVITY) => &mut self.pbs,
            Some(RPN_FINE_TUNE) => &mut self.fine_tune,
            Some(RPN_COARSE_TUNE) => &mut self.coarse_tune,
            _ => return,
        };
        slot.get_or_insert_with(RpnDataEntry::default).lsb = Some(val);
    }

    /// 输出该通道的 chase 重放消息（通道无任何已记录状态时返回空）
    pub(crate) fn emit(&self, channel: u8) -> Vec<MidiMessage> {
        if self.cc_seen == 0 && self.program.is_none() && self.pitch_bend.is_none() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(16);
        let mut cc = |controller: u8, value: u8| {
            out.push(MidiMessage::ControlChange {
                channel,
                controller,
                value,
            });
        };

        // 1. RPN 参数完整序列（选择 → Data Entry），保证合成器按序解析
        let rpn_entries = [
            (RPN_PITCH_BEND_SENSITIVITY, self.pbs),
            (RPN_FINE_TUNE, self.fine_tune),
            (RPN_COARSE_TUNE, self.coarse_tune),
        ];
        for (param_id, entry) in rpn_entries
            .into_iter()
            .filter_map(|(id, entry)| entry.map(|e| (id, e)))
        {
            cc(101, 0);
            cc(100, param_id);
            cc(6, entry.msb);
            if let Some(lsb) = entry.lsb {
                cc(38, lsb);
            }
        }

        // 2. 其余 CC 按号升序（bank select CC0/32 先于后续 ProgramChange）
        for idx in 0..128usize {
            if self.cc_seen & (1u128 << idx) != 0 && !matches!(idx, 6 | 38 | 100 | 101) {
                cc(idx as u8, self.ccs[idx]);
            }
        }

        // 3. 音色与弯音
        if let Some(program) = self.program {
            out.push(MidiMessage::ProgramChange { channel, program });
        }
        if let Some(value) = self.pitch_bend {
            out.push(MidiMessage::PitchBend { channel, value });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(controller: u8, value: u8) -> u16 {
        (u16::from(controller) << 8) | u16::from(value)
    }

    #[test]
    fn test_cc_latest_wins() {
        let mut state = ChannelChase::default();
        state.apply(0, cc(7, 40));
        state.apply(0, cc(10, 64));
        state.apply(0, cc(7, 100)); // 覆盖旧值
        let msgs = state.emit(0);
        let cc7: Vec<_> = msgs
            .iter()
            .filter_map(|m| match m {
                MidiMessage::ControlChange {
                    controller: 7,
                    value,
                    ..
                } => Some(*value),
                _ => None,
            })
            .collect();
        assert_eq!(cc7, vec![100], "CC7 应只重放最新值");
        assert!(
            msgs.iter()
                .any(|m| matches!(m, MidiMessage::ControlChange { controller: 10, .. })),
            "CC10 应保留"
        );
    }

    #[test]
    fn test_program_and_pitch_bend() {
        let mut state = ChannelChase::default();
        state.apply(1, 42); // PC 42
        state.apply(2, 8192 + 4096); // 弯音 +0.5
        let msgs = state.emit(3);
        assert_eq!(channel_of(&msgs), [3, 3]);
        assert!(matches!(
            &msgs[0],
            MidiMessage::ProgramChange { program: 42, .. }
        ));
        match &msgs[1] {
            MidiMessage::PitchBend { value, .. } => {
                assert!((*value - 0.5).abs() < 0.001, "弯音应≈+0.5，实际 {value}");
            }
            other => panic!("应为 PitchBend，实际 {other:?}"),
        }
    }

    #[test]
    fn test_rpn_sequences_replayed_in_order() {
        let mut state = ChannelChase::default();
        // 先设置 PBS=12.5 半音（MSB=12, LSB=50）
        state.apply(0, cc(101, 0));
        state.apply(0, cc(100, 0));
        state.apply(0, cc(6, 12));
        state.apply(0, cc(38, 50));
        // 再设置 Coarse Tune = -24（MSB=64-24=40）
        state.apply(0, cc(101, 0));
        state.apply(0, cc(100, 2));
        state.apply(0, cc(6, 40));

        let msgs = state.emit(0);
        let seq: Vec<(u8, u8)> = msgs
            .iter()
            .filter_map(|m| match m {
                MidiMessage::ControlChange {
                    controller, value, ..
                } => Some((*controller, *value)),
                _ => None,
            })
            .collect();
        // 期望两个完整序列：PBS（含 LSB），Coarse（无 LSB——第二次未发 CC38）
        assert_eq!(
            seq,
            vec![
                (101, 0),
                (100, 0),
                (6, 12),
                (38, 50), // PBS 序列
                (101, 0),
                (100, 2),
                (6, 40), // Coarse 序列
            ],
            "RPN 应按参数顺序输出完整选择+写入序列"
        );
    }

    #[test]
    fn test_rpn_selection_reset_by_nonzero_msb() {
        let mut state = ChannelChase::default();
        state.apply(0, cc(101, 0));
        state.apply(0, cc(100, 0));
        state.apply(0, cc(6, 12)); // PBS MSB=12
        // 切换到 null RPN（MSB=127）后写 DE 不应污染 PBS
        state.apply(0, cc(101, 127));
        state.apply(0, cc(100, 127));
        state.apply(0, cc(6, 99));
        let msgs = state.emit(0);
        let de6: Vec<u8> = msgs
            .iter()
            .filter_map(|m| match m {
                MidiMessage::ControlChange {
                    controller: 6,
                    value,
                    ..
                } => Some(*value),
                _ => None,
            })
            .collect();
        assert_eq!(de6, vec![12], "null RPN 期间的 DE 写入不应产生新序列");
    }

    #[test]
    fn test_sustain_restored_via_generic_cc() {
        let mut state = ChannelChase::default();
        state.apply(0, cc(64, 127)); // 踏板踩下
        let msgs = state.emit(0);
        assert!(
            msgs.iter().any(|m| matches!(
                m,
                MidiMessage::ControlChange {
                    controller: 64,
                    value: 127,
                    ..
                }
            )),
            "seek 后 sustain 踩下状态应被重放"
        );
    }

    #[test]
    fn test_empty_channel_emits_nothing() {
        let state = ChannelChase::default();
        assert!(state.emit(0).is_empty(), "无状态的通道不应产生 chase 消息");
    }

    #[test]
    fn test_apply_message_roundtrip() {
        let mut state = ChannelChase::default();
        state.apply_message(&MidiMessage::ControlChange {
            channel: 0,
            controller: 7,
            value: 90,
        });
        state.apply_message(&MidiMessage::PitchBend {
            channel: 0,
            value: -1.0,
        });
        let msgs = state.emit(0);
        assert!(matches!(
            &msgs[0],
            MidiMessage::ControlChange {
                controller: 7,
                value: 90,
                ..
            }
        ));
        match &msgs[1] {
            MidiMessage::PitchBend { value, .. } => {
                assert!((*value + 1.0).abs() < 0.01, "-1.0 往返应保持，实际 {value}")
            }
            other => panic!("应为 PitchBend，实际 {other:?}"),
        }
    }

    fn channel_of(msgs: &[MidiMessage]) -> Vec<u8> {
        msgs.iter().map(channel_of_msg).collect()
    }

    fn channel_of_msg(m: &MidiMessage) -> u8 {
        match m {
            MidiMessage::ControlChange { channel, .. }
            | MidiMessage::ProgramChange { channel, .. }
            | MidiMessage::PitchBend { channel, .. } => *channel,
            _ => unreachable!("测试只产生控制类消息"),
        }
    }
}
