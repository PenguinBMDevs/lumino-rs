//! MIDI 事件提取工具
//!
//! 从 `MidiDocument` 中按需提取各类事件，供 Runner 保存/导出复用。

use std::collections::HashMap;

use lumino_midi_loader::MidiDocument;

use super::{MidiControlChangeEvent, MidiProgramChangeEvent};

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
