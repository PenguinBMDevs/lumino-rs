//! 事件类型转换 — MergedEvent ↔ TrackEventKind 转换及辅助函数

use midly::num::{u4, u7, u14};
use midly::{MidiMessage, PitchBend, TrackEventKind};

use lumino_midi_loader::MidiDocument;

use super::event_stream::MergedEvent;

/// 将 MergedEvent 转换为 TrackEventKind；遇到未知 kind 返回 None（跳过该事件）
pub(super) fn build_track_event_kind(event: &MergedEvent) -> Option<TrackEventKind<'static>> {
    let channel = u4::new(event.channel & 0x0f);
    match event.kind {
        0 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::NoteOn {
                key: event.param1,
                vel: u7::new(event.param2 as u8),
            },
        }),
        1 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::NoteOff {
                key: event.param1,
                vel: u7::new(0),
            },
        }),
        2 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::Controller {
                controller: u7::new(event.param1),
                value: u7::new(event.param2 as u8),
            },
        }),
        3 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::ProgramChange {
                program: u7::new(event.param1),
            },
        }),
        4 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::PitchBend {
                bend: PitchBend(u14::new(event.param2)),
            },
        }),
        _ => None,
    }
}

/// 计算文档的总 tick 数（以最后音符的 end_tick 为准）
pub(super) fn compute_total_tick(doc: &MidiDocument) -> u64 {
    doc.notes
        .iter()
        .flat_map(|t| t.iter())
        .map(|n| n.end_tick as u64)
        .max()
        .unwrap_or(0)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_model::{NoteEvent, track::TrackManager};

    fn make_doc(notes: Vec<Vec<NoteEvent>>, total_ticks: u32) -> MidiDocument {
        let track_count = notes.len() as u16;
        MidiDocument {
            notes: notes
                .into_iter()
                .map(lumino_midi_model::ChunkedList::from_sorted)
                .collect(),
            time_signatures: vec![(0, 4, 4)],
            tempo_changes: vec![(0, 120.0)],
            key_signatures: vec![(0, 0, false)],
            control_events: lumino_midi_model::ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: (0..track_count).map(|_| None).collect(),
            total_ticks,
            track_count,
            tracks: TrackManager::new(track_count),
            division: 480,
            track_ports: vec![],
        }
    }

    #[test]
    fn test_compute_total_tick() {
        let doc = make_doc(vec![vec![NoteEvent::new(0, 100, 60, 100, 0)]], 100);
        assert_eq!(compute_total_tick(&doc), 100);
    }
}
