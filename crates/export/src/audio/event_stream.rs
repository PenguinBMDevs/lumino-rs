//! MIDI 文档事件流迭代器 — 将 MidiDocument 中的音符和控制事件合并为按 tick 排序的事件流
//!
//! 参考 OmniConverter 的 MIDIConverter 设计：
//! - 多轨道事件合并（跨轨最小 tick 优先）
//! - 相同 tick 按优先级排序（NoteOff > CC > PC > PB > NoteOn）

use lumino_midi_loader::MidiDocument;

/// 合并事件（8 字节对齐）
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub(crate) struct MergedEvent {
    pub(crate) tick: u32,
    /// 0=NoteOn, 1=NoteOff, 2=CC, 3=PC, 4=PB
    pub(crate) kind: u8,
    pub(crate) channel: u8,
    pub(crate) param1: u8,
    pub(crate) param2: u16,
}

/// MidiDocEventStream — 流式迭代 MidiDocument 中的事件
pub(crate) struct MidiDocEventStream<'a> {
    doc: &'a MidiDocument,
    note_cursors: Vec<(usize, bool)>,
    ctrl_cursor: usize,
    total_events: usize,
    emitted: usize,
}

impl<'a> MidiDocEventStream<'a> {
    pub fn new(doc: &'a MidiDocument) -> Self {
        let track_count = doc.notes.len();
        let note_cursors = vec![(0_usize, false); track_count];
        let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
        let total_events = total_notes * 2 + doc.control_events.len();
        MidiDocEventStream {
            doc,
            note_cursors,
            ctrl_cursor: 0,
            total_events,
            emitted: 0,
        }
    }

    pub fn total_events(&self) -> usize {
        self.total_events
    }

    /// 在所有游标中找到最小 tick
    fn find_min_tick(&self) -> u32 {
        let mut min_tick = u32::MAX;

        for (track_idx, &(note_idx, note_on_emitted)) in self.note_cursors.iter().enumerate() {
            if note_idx < self.doc.notes[track_idx].len() {
                let note = &self.doc.notes[track_idx][note_idx];
                let tick = if note_on_emitted {
                    note.end_tick
                } else {
                    note.start_tick
                };
                if tick < min_tick {
                    min_tick = tick;
                }
            }
        }
        if self.ctrl_cursor < self.doc.control_events.len() {
            let tick = self.doc.control_events[self.ctrl_cursor].tick;
            if tick < min_tick {
                min_tick = tick;
            }
        }

        min_tick
    }

    /// 在指定 tick 处找到优先级最高的事件（priority 数值越小优先级越高）
    fn find_best_event_at(&self, min_tick: u32) -> Option<(u8, MergedEvent)> {
        let mut best: Option<(u8, MergedEvent)> = None;

        // 扫描所有音轨，找最小 tick 处的事件
        for (track_idx, &(note_idx, note_on_emitted)) in self.note_cursors.iter().enumerate() {
            if note_idx >= self.doc.notes[track_idx].len() {
                continue;
            }
            let note = &self.doc.notes[track_idx][note_idx];
            let tick = if note_on_emitted {
                note.end_tick
            } else {
                note.start_tick
            };
            if tick != min_tick {
                continue;
            }
            let priority = if note_on_emitted { 1 } else { 5 };
            let event = if note_on_emitted {
                MergedEvent {
                    tick: note.end_tick,
                    kind: 1,
                    channel: note.channel,
                    param1: note.key,
                    param2: 0,
                }
            } else {
                MergedEvent {
                    tick: note.start_tick,
                    kind: 0,
                    channel: note.channel,
                    param1: note.key,
                    param2: note.velocity as u16,
                }
            };
            if best.as_ref().is_none_or(|(p, _)| priority < *p) {
                best = Some((priority, event));
            }
        }

        // 扫描控制事件
        self.try_control_event_at(min_tick, &mut best);

        best
    }

    /// 尝试在指定 tick 处添加控制事件到最佳候选
    fn try_control_event_at(&self, min_tick: u32, best: &mut Option<(u8, MergedEvent)>) {
        if self.ctrl_cursor >= self.doc.control_events.len() {
            return;
        }
        let ctrl = &self.doc.control_events[self.ctrl_cursor];
        if ctrl.tick != min_tick {
            return;
        }
        let candidate = match ctrl.kind {
            0 => {
                let (c, v) = ctrl.as_control_change();
                Some((
                    2,
                    MergedEvent {
                        tick: ctrl.tick,
                        kind: 2,
                        channel: ctrl.channel,
                        param1: c,
                        param2: v as u16,
                    },
                ))
            }
            1 => Some((
                3,
                MergedEvent {
                    tick: ctrl.tick,
                    kind: 3,
                    channel: ctrl.channel,
                    param1: ctrl.as_program_change(),
                    param2: 0,
                },
            )),
            2 => Some((
                4,
                MergedEvent {
                    tick: ctrl.tick,
                    kind: 4,
                    channel: ctrl.channel,
                    param1: 0,
                    param2: ctrl.param,
                },
            )),
            _ => None,
        };

        if let Some(cand) = candidate
            && best.as_ref().is_none_or(|(p, _)| cand.0 < *p)
        {
            *best = Some(cand);
        }
    }

    /// 根据发出的事件推进游标
    fn advance_cursors(&mut self, event: &MergedEvent) {
        match event.kind {
            0 | 1 => {
                for (track_idx, cursor) in self.note_cursors.iter_mut().enumerate() {
                    let (note_idx, note_on_emitted) = cursor;
                    if *note_idx < self.doc.notes[track_idx].len() {
                        let note = &self.doc.notes[track_idx][*note_idx];
                        let note_tick = if *note_on_emitted {
                            note.end_tick
                        } else {
                            note.start_tick
                        };
                        if note_tick == event.tick {
                            if *note_on_emitted {
                                *note_idx += 1;
                                *note_on_emitted = false;
                            } else {
                                *note_on_emitted = true;
                            }
                            break;
                        }
                    }
                }
            }
            2..=4 => {
                self.ctrl_cursor += 1;
            }
            _ => {}
        }
    }

    /// 获取下一个事件
    pub fn next_event(&mut self) -> Option<MergedEvent> {
        if self.emitted >= self.total_events {
            return None;
        }

        let min_tick = self.find_min_tick();
        if min_tick == u32::MAX {
            return None;
        }

        let best = self.find_best_event_at(min_tick);

        if let Some((_, ref event)) = best {
            self.advance_cursors(event);
        }

        self.emitted += 1;
        best.map(|(_, e)| e)
    }
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
    fn test_next_event_single_note_order() {
        let doc = make_doc(vec![vec![NoteEvent::new(0, 10, 60, 100, 0)]], 10);
        let mut stream = MidiDocEventStream::new(&doc);
        assert_eq!(stream.total_events(), 2, "1 note = 2 events (on+off)");

        let e1 = stream.next_event().expect("first event");
        assert_eq!(e1.kind, 0, "first should be NoteOn (kind=0)");
        assert_eq!(e1.tick, 0, "note-on at start tick");
        assert_eq!(e1.channel, 0);
        assert_eq!(e1.param1, 60, "param1 = note key");

        let e2 = stream.next_event().expect("second event");
        assert_eq!(e2.kind, 1, "second should be NoteOff (kind=1)");
        assert_eq!(e2.tick, 10, "note-off at end tick");
        assert_eq!(e2.channel, 0);
        assert_eq!(e2.param1, 60, "param1 = note key");

        assert!(stream.next_event().is_none(), "stream should be exhausted");
    }

    #[test]
    fn test_next_event_cross_track_min_tick() {
        let doc = make_doc(
            vec![
                vec![NoteEvent::new(10, 20, 60, 100, 0)],
                vec![NoteEvent::new(0, 5, 64, 100, 1)],
            ],
            20,
        );
        let mut stream = MidiDocEventStream::new(&doc);
        let e = stream.next_event().expect("first event");
        assert_eq!(e.tick, 0, "first event should be at earliest tick");
        assert_eq!(e.kind, 0, "first event should be NoteOn (kind=0)");
        assert_eq!(e.channel, 1, "track 1 has the earlier note");
        assert_eq!(e.param1, 64, "key from track 1");
    }

    #[test]
    fn test_next_event_tie_same_tick_first_track() {
        let doc = make_doc(
            vec![
                vec![NoteEvent::new(0, 10, 60, 100, 0)],
                vec![NoteEvent::new(0, 10, 64, 100, 1)],
            ],
            10,
        );
        let mut stream = MidiDocEventStream::new(&doc);
        let e = stream.next_event().expect("first event");
        assert_eq!(e.channel, 0, "track 0 wins tie (iteration order)");
        let e = stream.next_event().expect("second event");
        assert_eq!(e.kind, 0, "second event should also be NoteOn (kind=0)");
        assert_eq!(e.channel, 1, "track 1 second");
    }
}
