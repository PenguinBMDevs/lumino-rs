//! MIDI 事件提取工具
//!
//! 从 `MidiDocument` 中按需提取各类事件，供 Runner 保存/导出复用。

use std::collections::{HashMap, VecDeque};

use lumino_midi_loader::MidiDocument;

use super::{
    MidiChannelAftertouchEvent, MidiControlChangeEvent, MidiKeySignatureEvent, MidiLyricEvent,
    MidiMarkerEvent, MidiPitchBendEvent, MidiPolyAftertouchEvent, MidiProgramChangeEvent,
    MidiSysExEvent, MidiTextMetaEvent,
};

/// 从 `MidiDocument.control_events` 按轨提取 PC/CC 事件。
///
/// 返回 `(program_changes, control_changes)` 按轨索引分组的 HashMap。
pub fn extract_pc_cc_events(
    doc: &MidiDocument,
) -> (
    HashMap<u16, Vec<MidiProgramChangeEvent>>,
    HashMap<u16, Vec<MidiControlChangeEvent>>,
) {
    let mut pc_by_track: HashMap<u16, Vec<MidiProgramChangeEvent>> = HashMap::new();
    let mut cc_by_track: HashMap<u16, Vec<MidiControlChangeEvent>> = HashMap::new();

    for ev in &doc.control_events {
        match ev.kind {
            0 => {
                // Control Change
                let (controller, value) = ev.as_control_change();
                cc_by_track
                    .entry(ev.track)
                    .or_default()
                    .push(MidiControlChangeEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        controller,
                        value,
                    });
            }
            1 => {
                // Program Change
                let program = ev.as_program_change();
                pc_by_track
                    .entry(ev.track)
                    .or_default()
                    .push(MidiProgramChangeEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        program,
                    });
            }
            _ => {} // Pitch Bend and others — not exported as PC/CC
        }
    }

    (pc_by_track, cc_by_track)
}

/// 文档透传事件（保存=无损往返：编辑器未建模的事件原样带回）
///
/// 按轨分组，调用方按轨填入 `MidiTrackData`；`key_signatures` 为全局事件，
/// 调用方放到首轨（与 tempo/拍号一致）。
#[derive(Debug, Default)]
pub struct DocPassthrough {
    /// 弯音（按轨）
    pub pitch_bends: HashMap<u16, Vec<MidiPitchBendEvent>>,
    /// 通道触后（按轨）
    pub channel_aftertouch: HashMap<u16, Vec<MidiChannelAftertouchEvent>>,
    /// 复音触后（按轨）
    pub poly_aftertouch: HashMap<u16, Vec<MidiPolyAftertouchEvent>>,
    /// 歌词（按轨）
    pub lyrics: HashMap<u16, Vec<MidiLyricEvent>>,
    /// 标记（按轨）
    pub markers: HashMap<u16, Vec<MidiMarkerEvent>>,
    /// 文本类元事件（按轨）
    pub text_events: HashMap<u16, Vec<MidiTextMetaEvent>>,
    /// SysEx（按轨）
    pub sys_ex: HashMap<u16, Vec<MidiSysExEvent>>,
    /// MIDI 端口（按轨，仅端口非 0 的轨道记录，0=默认值不写）
    pub midi_ports: HashMap<u16, u8>,
    /// 调号（全局）
    pub key_signatures: Vec<MidiKeySignatureEvent>,
}

/// 从 `MidiDocument` 提取编辑器未建模、需透传的全部事件。
pub fn extract_passthrough_events(doc: &MidiDocument) -> DocPassthrough {
    let mut out = DocPassthrough::default();

    for ev in &doc.control_events {
        match ev.kind {
            2 => {
                let bend = (ev.as_pitch_bend() * 8192.0).round() as i16;
                out.pitch_bends
                    .entry(ev.track)
                    .or_default()
                    .push(MidiPitchBendEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        value: bend.saturating_add(8192).clamp(0, 16383) as u16,
                    });
            }
            3 => {
                out.channel_aftertouch.entry(ev.track).or_default().push(
                    MidiChannelAftertouchEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        velocity: ev.as_channel_aftertouch(),
                    },
                );
            }
            4 => {
                let (key, velocity) = ev.as_poly_aftertouch();
                out.poly_aftertouch
                    .entry(ev.track)
                    .or_default()
                    .push(MidiPolyAftertouchEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        key,
                        velocity,
                    });
            }
            _ => {}
        }
    }

    for (tick, track, bytes) in &doc.lyrics {
        out.lyrics.entry(*track).or_default().push(MidiLyricEvent {
            tick: *tick,
            bytes: bytes.clone(),
        });
    }
    for (tick, track, bytes) in &doc.markers {
        out.markers
            .entry(*track)
            .or_default()
            .push(MidiMarkerEvent {
                tick: *tick,
                bytes: bytes.clone(),
            });
    }
    for (tick, track, meta_type, bytes) in &doc.text_events {
        out.text_events
            .entry(*track)
            .or_default()
            .push(MidiTextMetaEvent {
                tick: *tick,
                meta_type: *meta_type,
                bytes: bytes.clone(),
            });
    }
    for (tick, track, bytes) in &doc.sys_ex {
        out.sys_ex.entry(*track).or_default().push(MidiSysExEvent {
            tick: *tick,
            bytes: bytes.clone(),
        });
    }
    for (idx, port) in doc.track_ports.iter().enumerate() {
        if *port != 0 {
            out.midi_ports.insert(idx as u16, *port);
        }
    }
    // key_signatures: (tick, 升降号数, 是否小调) → 导出结构
    out.key_signatures = doc
        .key_signatures
        .iter()
        .map(|(tick, key, is_minor)| MidiKeySignatureEvent {
            tick: *tick,
            key: *key,
            is_major: !is_minor,
        })
        .collect();

    out
}

/// 释放力度匹配表：轨 → (tick, key, channel) → 待消费的释放力度队列。
///
/// 编辑器音符元组无释放力度概念；未编辑音符按 (tick,key,channel) 回查文档，
/// 逐个消费保证重叠同键音符一一对应。匹配失败回退 0（编辑过的新音符）。
pub type ReleaseMap = HashMap<u16, HashMap<(u32, u8, u8), VecDeque<u8>>>;

/// 从 `MidiDocument` 构建释放力度匹配表（按文档音符顺序入队）。
pub fn build_release_map(doc: &MidiDocument) -> ReleaseMap {
    let mut map: ReleaseMap = HashMap::new();
    for track_id in 0..doc.track_count {
        for note in doc.track_notes(track_id as usize).iter() {
            map.entry(track_id)
                .or_default()
                .entry((note.start_tick, note.key, note.channel))
                .or_default()
                .push_back(note.release_velocity);
        }
    }
    map
}
