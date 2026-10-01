//! seek / 循环回绕后的模态状态追齐（MIDI chase）
//!
//! CC / Program / Pitch Bend / RPN-NRPN 都是“当前值”模态：跳转到任意位置后，
//! 必须把该位置之前每个通道的最后状态重新发送给输出，否则合成器会保留
//! 跳转前的旧状态（例如 RPN 微调停在旧值，直到后续事件再次覆盖）。
//!
//! 追齐数据来源为当前轨的 `midi_events`（automation lane 展开结果）与
//! 非当前轨的 `document.control_events`，两者按 tick 合并后取“每通道最后值”。
//! 输出顺序：Program → RPN/NRPN 选择（MSB→LSB）→ DataEntry（MSB→LSB）→
//! 其他 CC（控制器升序）→ Pitch Bend，保证 RPN 数据不会落到错误参数上。

use lumino_midi_model::multi_port::{PercussionTracker, channels_for_max_port_clamped};

use crate::playback::engine::types::global_channel_for_track;

use super::super::MidiMessage;
use super::core::PlaybackEngine;

/// 选择类控制器（98 = NRPN LSB，99 = NRPN MSB，100 = RPN LSB，101 = RPN MSB）。
const SELECT_CC: [u8; 4] = [98, 99, 100, 101];
/// Data Entry（6 = MSB，38 = LSB）。
const DATA_CC: [u8; 2] = [6, 38];

/// 单通道的追齐状态。
#[derive(Clone)]
struct ChannelChase {
    /// 每个控制器的最后值。
    cc: [Option<u8>; 128],
    /// 最后 Program Change。
    program: Option<u8>,
    /// 最后 Pitch Bend（归一化 -1.0..1.0）。
    pitch_bend: Option<f32>,
    /// 选择类控制器最后一次出现的 tick（下标对应 [`SELECT_CC`]）。
    select_tick: [Option<f32>; 4],
}

impl Default for ChannelChase {
    fn default() -> Self {
        Self {
            cc: [None; 128],
            program: None,
            pitch_bend: None,
            select_tick: [None; 4],
        }
    }
}

impl PlaybackEngine {
    /// 计算 `tick` 之前（严格小于）各通道的最终控制状态。
    ///
    /// 返回追齐消息与**该 tick 处的打击乐模态快照**；调用方（seek / 循环回绕）
    /// 必须用快照同步引擎内部跟踪器，否则后续 Bank Select 会与旧状态比较而漏切换。
    ///
    /// 严格小于保证处于 `tick` 的事件仍由正常播放路径发送，不重复也不遗漏。
    pub(crate) fn compute_chase(&self, tick: f32) -> (Vec<MidiMessage>, PercussionTracker) {
        let doc = self.document.as_deref();
        // REND-002：通道空间随文档端口数；无文档时回退 16（Midi 基线）。
        let channels = doc.map_or(16, |doc| channels_for_max_port_clamped(doc.max_port()));
        let mut states: Vec<ChannelChase> = vec![ChannelChase::default(); channels as usize];
        let mut percussion = PercussionTracker::new(channels);

        // 源 1：当前轨 midi_events（tick 升序）
        let midi_end = self.midi_events.partition_point(|event| event.tick < tick);
        // 源 2：非当前轨 document 控制事件（tick 升序）；严格小于 tick
        let doc_end = doc.map_or(0, |doc| {
            doc.control_events
                .partition_point(tick.max(0.0).ceil() as u32)
        });

        // 双指针按 tick 合并两个升序源，保证“每通道最后值”取到真正最后的事件。
        let mut i = 0usize;
        let mut j = 0usize;
        while i < midi_end || j < doc_end {
            let m_tick = self.midi_events.get(i).map(|event| event.tick);
            let d_tick = doc
                .and_then(|doc| doc.control_events.get(j))
                .map(|event| event.tick as f32);
            let take_midi = match (m_tick, d_tick) {
                (Some(_), Some(d)) => m_tick.is_some_and(|m| m <= d),
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if take_midi {
                if let Some(event) = self.midi_events.get(i) {
                    apply_message(&mut states, &mut percussion, event.tick, &event.message);
                }
                i += 1;
            } else if let Some(event) = doc.and_then(|doc| doc.control_events.get(j)) {
                let port = doc.map_or(0, |doc| doc.track_port(event.track));
                apply_document_event(&mut states, &mut percussion, event, port);
                j += 1;
            }
        }

        (emit_chase(&states, &percussion), percussion)
    }

    /// 取走待发送的追齐消息（seek 时填充，播放线程在命令处理后发送）。
    pub(crate) fn take_pending_chase(&mut self) -> Vec<MidiMessage> {
        std::mem::take(&mut self.pending_chase)
    }
}

/// 应用一条当前轨事件到追齐状态。
fn apply_message(
    states: &mut [ChannelChase],
    percussion: &mut PercussionTracker,
    tick: f32,
    message: &MidiMessage,
) {
    match message {
        MidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => {
            let ch = *channel as usize;
            if ch >= states.len() {
                return;
            }
            states[ch].cc[*controller as usize] = Some(*value);
            if let Some(idx) = SELECT_CC.iter().position(|c| c == controller) {
                states[ch].select_tick[idx] = Some(tick);
            }
            // REND-002 方案 B：Bank Select 参与打击乐模态追齐。
            let _ = percussion.observe_cc(*channel, *controller, *value);
        }
        MidiMessage::ProgramChange { channel, program } => {
            let ch = *channel as usize;
            if ch < states.len() {
                states[ch].program = Some(*program);
            }
        }
        MidiMessage::PitchBend { channel, value } => {
            let ch = *channel as usize;
            if ch < states.len() {
                states[ch].pitch_bend = Some(*value);
            }
        }
        // `PercussionMode` 为派生消息，不作为追齐输入（由 tracker 统一推导）。
        _ => {}
    }
}

/// 应用一条 document 控制事件到追齐状态（`port` 为来源轨道端口）。
fn apply_document_event(
    states: &mut [ChannelChase],
    percussion: &mut PercussionTracker,
    event: &midly::loader::PackedControlEvent,
    port: u8,
) {
    let channel = global_channel_for_track(port, event.channel);
    let ch = channel as usize;
    if ch >= states.len() {
        return;
    }
    match event.kind {
        0 => {
            let (controller, value) = event.as_control_change();
            states[ch].cc[controller as usize] = Some(value);
            if let Some(idx) = SELECT_CC.iter().position(|c| *c == controller) {
                states[ch].select_tick[idx] = Some(event.tick as f32);
            }
            let _ = percussion.observe_cc(channel, controller, value);
        }
        1 => states[ch].program = Some(event.as_program_change()),
        2 => states[ch].pitch_bend = Some(event.as_pitch_bend()),
        _ => {}
    }
}

/// 将追齐状态展开为消息序列。
fn emit_chase(states: &[ChannelChase], percussion: &PercussionTracker) -> Vec<MidiMessage> {
    let mut out = Vec::new();
    for (ch, state) in states.iter().enumerate() {
        let ch = ch as u16;
        // REND-002 方案 B：有 Bank Select 证据的通道先追齐打击乐模态，
        // 保证 seek 后的鼓组/旋律库选择落在正确 bank 上。
        if percussion.has_evidence(ch) {
            out.push(MidiMessage::PercussionMode {
                channel: ch,
                on: percussion.is_percussion(ch),
            });
        }
        let has_any = state.program.is_some()
            || state.pitch_bend.is_some()
            || state.cc.iter().any(Option::is_some);
        if !has_any {
            continue;
        }
        if let Some(program) = state.program {
            out.push(MidiMessage::ProgramChange {
                channel: ch,
                program,
            });
        }
        // 只追“最后一次被选中”的 RPN/NRPN 家族，避免两个家族的残留值互相覆盖。
        let last_select = state
            .select_tick
            .iter()
            .enumerate()
            .filter_map(|(idx, &tick)| tick.map(|tick| (idx, tick)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((idx, _)) = last_select {
            // 0/1 = NRPN，2/3 = RPN；先 MSB 再 LSB。
            let (msb_idx, lsb_idx) = if idx >= 2 { (3, 2) } else { (1, 0) };
            for sel_idx in [msb_idx, lsb_idx] {
                let controller = SELECT_CC[sel_idx];
                if let Some(value) = state.cc[controller as usize] {
                    out.push(MidiMessage::ControlChange {
                        channel: ch,
                        controller,
                        value,
                    });
                }
            }
            // DataEntry 仅在存在选择状态时追齐（否则目标参数不明确）。
            for controller in DATA_CC {
                if let Some(value) = state.cc[controller as usize] {
                    out.push(MidiMessage::ControlChange {
                        channel: ch,
                        controller,
                        value,
                    });
                }
            }
        }
        // 其他 CC（控制器升序，跳过选择与数据字节）。
        for controller in 0u8..=127 {
            if SELECT_CC.contains(&controller) || DATA_CC.contains(&controller) {
                continue;
            }
            if let Some(value) = state.cc[controller as usize] {
                out.push(MidiMessage::ControlChange {
                    channel: ch,
                    controller,
                    value,
                });
            }
        }
        if let Some(value) = state.pitch_bend {
            out.push(MidiMessage::PitchBend { channel: ch, value });
        }
    }
    out
}

#[cfg(test)]
mod tests;
