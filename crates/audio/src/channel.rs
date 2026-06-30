//! MIDI 通道状态 — 用于 seek 后的 Chase（重放控制器状态）。
//!
//! 追踪每个通道的 CC/PC/PB/RPN 值，seek 时重放以恢复正确音色。
//! 只使用 lumino xsynth fork 支持的 API（Raw CC + PitchBendValue + ProgramChange）。

use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent, ControlEvent};
use xsynth_core::channel_group::{ChannelGroup, SynthEvent};

/// 单个 MIDI 通道的完整控制器状态快照。
#[derive(Clone, Copy)]
pub(crate) struct ChannelState {
    pub(crate) bank_msb: u8,
    pub(crate) bank_lsb: u8,
    pub(crate) program: u8,
    pub(crate) volume: u8,
    pub(crate) pan: u8,
    pub(crate) expression: u8,
    pub(crate) sustain: u8,
    pub(crate) pitch_bend: f32,
    pub(crate) cc_values: [u8; 128],
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            bank_msb: 0,
            bank_lsb: 0,
            program: 0,
            volume: 127,
            pan: 64,
            expression: 127,
            sustain: 0,
            pitch_bend: 0.0,
            cc_values: [0; 128],
        }
    }
}

impl ChannelState {
    /// 从一个控制事件更新状态。
    pub(crate) fn apply(&mut self, event: &ChannelAudioEvent) {
        match event {
            ChannelAudioEvent::Control(ControlEvent::Raw(cc, val)) => {
                let cc_idx = *cc as usize;
                if cc_idx < 128 {
                    self.cc_values[cc_idx] = *val;
                }
                match cc {
                    0 => self.bank_msb = *val,
                    7 => self.volume = *val,
                    10 => self.pan = *val,
                    11 => self.expression = *val,
                    32 => self.bank_lsb = *val,
                    64 => self.sustain = *val,
                    _ => {}
                }
            }
            ChannelAudioEvent::Control(ControlEvent::PitchBendValue(v)) => self.pitch_bend = *v,
            ChannelAudioEvent::ProgramChange(p) => self.program = *p,
            _ => {}
        }
    }

    /// 将状态发送到 ChannelGroup（用于 seek 后恢复）。
    ///
    /// 只发送 lumino xsynth fork 支持的事件类型。
    pub(crate) fn send_to(&self, ch: u32, cg: &mut ChannelGroup) {
        let mut send = |event: ChannelAudioEvent| {
            cg.send_event(SynthEvent::Channel(ch, ChannelEvent::Audio(event)));
        };

        // 发送所有已记录的 CC 值（包括 bank select, volume, pan 等）
        // 按 CC 编号顺序发送，确保 bank select 在 program change 之前
        send(ChannelAudioEvent::Control(ControlEvent::Raw(0, self.bank_msb)));
        send(ChannelAudioEvent::Control(ControlEvent::Raw(32, self.bank_lsb)));

        // 发送其他 CC（跳过 0 和 32，已单独发送）
        for cc in 1..128u8 {
            if cc == 32 {
                continue;
            }
            let val = self.cc_values[cc as usize];
            if val != 0 {
                send(ChannelAudioEvent::Control(ControlEvent::Raw(cc, val)));
            }
        }

        // ProgramChange 在 CC 之后发送
        send(ChannelAudioEvent::ProgramChange(self.program));

        // PitchBend 最后发送
        send(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
            self.pitch_bend,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_state_apply() {
        let mut state = ChannelState::default();
        state.apply(&ChannelAudioEvent::Control(ControlEvent::Raw(7, 100)));
        assert_eq!(state.volume, 100);

        state.apply(&ChannelAudioEvent::Control(ControlEvent::Raw(10, 64)));
        assert_eq!(state.pan, 64);

        state.apply(&ChannelAudioEvent::ProgramChange(42));
        assert_eq!(state.program, 42);

        state.apply(&ChannelAudioEvent::Control(ControlEvent::PitchBendValue(0.5)));
        assert!((state.pitch_bend - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_default_values() {
        let state = ChannelState::default();
        assert_eq!(state.volume, 127);
        assert_eq!(state.pan, 64);
        assert_eq!(state.expression, 127);
        assert_eq!(state.program, 0);
        assert!(state.pitch_bend.abs() < f32::EPSILON);
    }
}
