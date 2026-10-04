//! seek / 循环回绕后的模态状态追齐（MIDI chase）
//!
//! CC / Program / Pitch Bend / RPN-NRPN 都是“当前值”模态：跳转到任意位置后，
//! 必须把该位置之前每个通道的最后状态重新发送给输出，否则合成器会保留
//! 跳转前的旧状态（例如 RPN 微调停在旧值，直到后续事件再次覆盖）。
//!
//! 追齐数据来源为当前轨的 `midi_events`（automation lane 展开结果）与
//! 非当前轨的 `document.control_events`，两者按 tick 合并后取“每通道最后值”。
//! RPN/NRPN 按引擎状态机复现（REND-014 #113）：三个 RPN 族（0/0 灵敏度、
//! 0/1 细调音、0/2 粗调音）的最终生效值全部追齐，且 DataEntry 归属“事件到达
//! 时”已选中的族——同 tick 数据先于选择时与实时文件序保持一致。
//! 输出顺序：打击乐模态 → Program → RPN 选择+数据（0/0→0/1→0/2）→ 恢复实时
//! 选择态 → 其他 CC（控制器升序）→ Pitch Bend。
//!
//! 契约：`ChannelPressure` / `PolyPressure`（后触）**不参与追齐**——文档控制事件
//! 只展开 kind 0/1/2（CC/PC/PB，见 `state.rs::push_control_event`），后触在 seek
//! 后保留合成器侧旧值；如需追齐，需先给 loader/render 补后触事件展开。

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
    /// 当前 RPN 选择（MSB, LSB）；`None` = 未选择或 NRPN（数据被消费）。
    rpn_sel: Option<(u8, u8)>,
    /// 最后一次选择是否为 NRPN（用于恢复选择态；NRPN 数据不生效）。
    sel_is_nrpn: bool,
    /// RPN 0/0（弯音灵敏度）DataEntry 字节 (MSB, LSB) + 是否生效过。
    rpn0: (u8, u8),
    rpn0_touched: bool,
    /// RPN 0/1（通道细调音）DataEntry 字节 (MSB, LSB) + 是否生效过。
    rpn1: (u8, u8),
    rpn1_touched: bool,
    /// RPN 0/2（通道粗调音，仅 MSB 生效）+ 是否生效过。
    rpn2: u8,
    rpn2_touched: bool,
}

impl Default for ChannelChase {
    fn default() -> Self {
        Self {
            cc: [None; 128],
            program: None,
            pitch_bend: None,
            rpn_sel: None,
            sel_is_nrpn: false,
            rpn0: (2, 0),
            rpn0_touched: false,
            rpn1: (64, 0),
            rpn1_touched: false,
            rpn2: 64,
            rpn2_touched: false,
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
                    apply_message(&mut states, &mut percussion, &event.message);
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

/// 应用一条控制事件到追齐状态（当前轨 `midi_events` 与 document 共用）。
///
/// RPN/NRPN 语义与合成器侧状态机一致（`gpu-synth` `ChannelState::handle_rpn_cc`
/// / fork `core/src/channel/control.rs`）：DataEntry 只作用于"事件到达时"已选中的
/// RPN 族——同 tick 内数据先于选择时归属旧族（REND-014 #113，与实时文件序一致）。
fn apply_control(state: &mut ChannelChase, controller: u8, value: u8) {
    state.cc[controller as usize] = Some(value);
    match controller {
        // NRPN 选择：引擎不实现 NRPN 效果，后续 DataEntry 被消费丢弃。
        0x62 | 0x63 => {
            state.rpn_sel = None;
            state.sel_is_nrpn = true;
        }
        0x64 => {
            let msb = state.rpn_sel.map_or(0, |(m, _)| m);
            state.rpn_sel = Some((msb, value));
            state.sel_is_nrpn = false;
        }
        0x65 => {
            let lsb = state.rpn_sel.map_or(0, |(_, l)| l);
            state.rpn_sel = Some((value, lsb));
            state.sel_is_nrpn = false;
        }
        0x06 | 0x26 => match state.rpn_sel {
            Some((0, 0)) => {
                if controller == 0x06 {
                    state.rpn0.0 = value;
                } else {
                    state.rpn0.1 = value;
                }
                state.rpn0_touched = true;
            }
            Some((0, 1)) => {
                if controller == 0x06 {
                    state.rpn1.0 = value;
                } else {
                    state.rpn1.1 = value;
                }
                state.rpn1_touched = true;
            }
            // 粗调音仅 MSB 生效（与引擎一致）。
            Some((0, 2)) if controller == 0x06 => {
                state.rpn2 = value;
                state.rpn2_touched = true;
            }
            _ => {}
        },
        _ => {}
    }
}

/// 应用一条当前轨事件到追齐状态。
fn apply_message(
    states: &mut [ChannelChase],
    percussion: &mut PercussionTracker,
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
            apply_control(&mut states[ch], *controller, *value);
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
            apply_control(&mut states[ch], controller, value);
            let _ = percussion.observe_cc(channel, controller, value);
        }
        1 => states[ch].program = Some(event.as_program_change()),
        2 => states[ch].pitch_bend = Some(event.as_pitch_bend()),
        _ => {}
    }
}

/// 追齐一条 RPN 族：选择（101=MSB、100=LSB）→ DataEntry（6=MSB、38=LSB）。
///
/// `lsb` 为 `None` 时只发 MSB（RPN 0/2 粗调音仅 MSB 生效，与引擎一致）。
fn push_rpn_family(out: &mut Vec<MidiMessage>, ch: u16, sel_lsb: u8, msb: u8, lsb: Option<u8>) {
    for (controller, value) in [(101u8, 0u8), (100, sel_lsb), (6, msb)] {
        out.push(MidiMessage::ControlChange {
            channel: ch,
            controller,
            value,
        });
    }
    if let Some(lsb) = lsb {
        out.push(MidiMessage::ControlChange {
            channel: ch,
            controller: 38,
            value: lsb,
        });
    }
}

/// 追齐 RPN 选择态（101=MSB、100=LSB）。
fn push_rpn_select(out: &mut Vec<MidiMessage>, ch: u16, msb: u8, lsb: u8) {
    for (controller, value) in [(101u8, msb), (100, lsb)] {
        out.push(MidiMessage::ControlChange {
            channel: ch,
            controller,
            value,
        });
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
            || state.cc.iter().any(Option::is_some)
            || state.rpn0_touched
            || state.rpn1_touched
            || state.rpn2_touched;
        if !has_any {
            continue;
        }
        if let Some(program) = state.program {
            out.push(MidiMessage::ProgramChange {
                channel: ch,
                program,
            });
        }
        // REND-014 #113：按引擎状态机复现三个 RPN 族的最终生效值（而非只追
        // “最后一次被选中的家族”）——同 tick 数据先于选择时数据归属旧族，
        // 只追最后族会丢掉此前已生效的细/粗调音或灵敏度状态，导致 seek 后
        // 与实时播放的首播行为不一致。
        let mut last_emitted_sel: Option<(u8, u8)> = None;
        if state.rpn0_touched {
            push_rpn_family(&mut out, ch, 0, state.rpn0.0, Some(state.rpn0.1));
            last_emitted_sel = Some((0, 0));
        }
        if state.rpn1_touched {
            push_rpn_family(&mut out, ch, 1, state.rpn1.0, Some(state.rpn1.1));
            last_emitted_sel = Some((0, 1));
        }
        if state.rpn2_touched {
            push_rpn_family(&mut out, ch, 2, state.rpn2, None);
            last_emitted_sel = Some((0, 2));
        }
        // 恢复实时选择态：否则 seek 后第一个 DataEntry 会落到追齐留下的最后一族。
        if let Some((msb, lsb)) = state.rpn_sel {
            if last_emitted_sel != Some((msb, lsb)) {
                push_rpn_select(&mut out, ch, msb, lsb);
            }
        } else if state.sel_is_nrpn
            && let (Some(msb), Some(lsb)) = (state.cc[99], state.cc[98])
        {
            out.push(MidiMessage::ControlChange {
                channel: ch,
                controller: 99,
                value: msb,
            });
            out.push(MidiMessage::ControlChange {
                channel: ch,
                controller: 98,
                value: lsb,
            });
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
