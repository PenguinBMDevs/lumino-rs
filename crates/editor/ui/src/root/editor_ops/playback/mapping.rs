//! 播放 MIDI 事件的通道映射与同 tick 排序（自 `playback.rs` 拆出，保持单文件 < 400 行）

use crate::playback::{MidiMessage, MidiTrackEvent};

/// REND-002：把消息的 MIDI 通道映射到合成层全局通道（端口来自来源轨道）。
pub(super) fn map_message_port(message: &MidiMessage, port: u8) -> MidiMessage {
    let map =
        |ch: u16| -> u16 { lumino_midi_model::multi_port::track_global_channel(port, ch as u8) };
    match message {
        MidiMessage::NoteOn {
            channel,
            key,
            velocity,
        } => MidiMessage::NoteOn {
            channel: map(*channel),
            key: *key,
            velocity: *velocity,
        },
        MidiMessage::NoteOff { channel, key } => MidiMessage::NoteOff {
            channel: map(*channel),
            key: *key,
        },
        MidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => MidiMessage::ControlChange {
            channel: map(*channel),
            controller: *controller,
            value: *value,
        },
        MidiMessage::ProgramChange { channel, program } => MidiMessage::ProgramChange {
            channel: map(*channel),
            program: *program,
        },
        MidiMessage::PitchBend { channel, value } => MidiMessage::PitchBend {
            channel: map(*channel),
            value: *value,
        },
        MidiMessage::ChannelPressure { channel, pressure } => MidiMessage::ChannelPressure {
            channel: map(*channel),
            pressure: *pressure,
        },
        MidiMessage::PolyPressure {
            channel,
            key,
            pressure,
        } => MidiMessage::PolyPressure {
            channel: map(*channel),
            key: *key,
            pressure: *pressure,
        },
        // 派生消息（模态切换）不携带来源轨道端口，原样保留。
        MidiMessage::PercussionMode { .. } => message.clone(),
    }
}

/// 同 tick 控制消息排序秩：RPN/NRPN 参数选择必须先于 DataEntry 生效。
///
/// - `0`：参数选择（CC98/99 = NRPN LSB/MSB，CC100/101 = RPN LSB/MSB）
/// - `2`：DataEntry（CC6 = MSB，CC38 = LSB）
/// - `1`：其他消息（不含选择/数据的普通 CC、PC、PB 等，保持既有相对顺序）
fn control_order_rank(message: &MidiMessage) -> u8 {
    match message {
        MidiMessage::ControlChange { controller, .. } => match controller {
            98..=101 => 0,
            6 | 38 => 2,
            _ => 1,
        },
        _ => 1,
    }
}

/// 按 tick 稳定排序 MIDI 事件；同 tick 内保证“选择 → 其他 → DataEntry”的次序。
///
/// `midi_events` 由多条来源拼装（automation lane / 预加载事件 / ProgramChange），
/// 跨 lane 汇总会按 lane 创建顺序重排同 tick 事件——例如文件先出现 NRPN 时，
/// CC6 lane 可能排在 RPN 的 CC101/100 lane 之前，导致 DataEntry 写到上一次
/// 选择的参数上。此处在同 tick 内施加最小次序约束；不同 tick 与无关消息的
/// 相对顺序保持不变（稳定排序）。
pub(super) fn sort_midi_events(events: &mut [MidiTrackEvent]) {
    events.sort_by(|a, b| {
        a.tick
            .total_cmp(&b.tick)
            .then_with(|| control_order_rank(&a.message).cmp(&control_order_rank(&b.message)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(tick: f32, controller: u8, value: u8) -> MidiTrackEvent {
        MidiTrackEvent {
            tick,
            message: MidiMessage::ControlChange {
                channel: 0,
                controller,
                value,
            },
        }
    }

    fn controllers(events: &[MidiTrackEvent]) -> Vec<u8> {
        events
            .iter()
            .map(|e| match &e.message {
                MidiMessage::ControlChange { controller, .. } => *controller,
                other => panic!("非 CC 消息: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn same_tick_selection_sorts_before_data_entry() {
        let mut events = vec![cc(96.0, 6, 64), cc(96.0, 101, 0), cc(96.0, 100, 1)];
        sort_midi_events(&mut events);
        assert_eq!(controllers(&events), vec![101, 100, 6]);
    }

    #[test]
    fn same_tick_unrelated_cc_order_preserved() {
        let mut events = vec![cc(0.0, 7, 100), cc(0.0, 10, 64)];
        sort_midi_events(&mut events);
        assert_eq!(controllers(&events), vec![7, 10]);
    }

    #[test]
    fn different_ticks_never_reordered() {
        let mut events = vec![cc(100.0, 6, 1), cc(0.0, 101, 0)];
        sort_midi_events(&mut events);
        assert_eq!(controllers(&events), vec![101, 6]);
        assert!(events[0].tick < events[1].tick);
    }

    #[test]
    fn rank_classifies_selection_and_data_entry() {
        for controller in [98u8, 99, 100, 101] {
            assert_eq!(control_order_rank(&cc(0.0, controller, 0).message), 0);
        }
        for controller in [6u8, 38] {
            assert_eq!(control_order_rank(&cc(0.0, controller, 0).message), 2);
        }
        assert_eq!(control_order_rank(&cc(0.0, 7, 0).message), 1);
        assert_eq!(
            control_order_rank(&MidiMessage::PitchBend {
                channel: 0,
                value: 0.0
            }),
            1
        );
    }
}
