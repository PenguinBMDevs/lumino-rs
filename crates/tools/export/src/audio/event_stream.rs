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
///
/// 将文档中所有 NoteOn/NoteOff 与控制事件展开为扁平列表后按 (tick, priority)
/// 排序，彻底解决旧游标实现中“同轨重叠音符屏蔽”问题：
/// 旧实现每轨仅跟踪一个 `(idx, bool)`，发射 `A On(0)` 后游标指向 `A Off(10)`，
/// 导致 `B On(5)` 被掩盖、推迟到 10 才触发（和弦变琶音）。新实现预排序保证
/// 时值精确，代价为一次 O(N log N) 排序（N=2*notes+controls，對 100K 音符约 200K 事件可接受）。
pub(crate) struct MidiDocEventStream {
    events: Vec<MergedEvent>,
    cursor: usize,
}

impl MidiDocEventStream {
    pub fn new(doc: &MidiDocument) -> Self {
        let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
        let total = total_notes * 2 + doc.control_events.len();
        let mut events = Vec::with_capacity(total);

        // 展开所有音符为 NoteOn/NoteOff
        for track_notes in doc.notes.iter() {
            for note in track_notes.iter() {
                events.push(MergedEvent {
                    tick: note.start_tick,
                    kind: 0,
                    channel: note.channel,
                    param1: note.key,
                    param2: note.velocity as u16,
                });
                events.push(MergedEvent {
                    tick: note.end_tick,
                    kind: 1,
                    channel: note.channel,
                    param1: note.key,
                    param2: 0,
                });
            }
        }
        // 展开控制事件
        for ctrl in doc.control_events.iter() {
            match ctrl.kind {
                0 => {
                    let (c, v) = ctrl.as_control_change();
                    events.push(MergedEvent {
                        tick: ctrl.tick,
                        kind: 2,
                        channel: ctrl.channel,
                        param1: c,
                        param2: v as u16,
                    });
                }
                1 => events.push(MergedEvent {
                    tick: ctrl.tick,
                    kind: 3,
                    channel: ctrl.channel,
                    param1: ctrl.as_program_change(),
                    param2: 0,
                }),
                2 => events.push(MergedEvent {
                    tick: ctrl.tick,
                    kind: 4,
                    channel: ctrl.channel,
                    param1: 0,
                    param2: ctrl.param,
                }),
                _ => {}
            }
        }

        // 按 (tick, priority) 稳定排序；priority 数值越小越先：NoteOff(1) > CC(2) > PC(3) > PB(4) > NoteOn(5)
        // 使用 stable sort 保持插入序（track 0 先于 track 1），与旧 find_best_event_at 的 tie 语义一致。
        events.sort_by(|a, b| {
            let pa = match a.kind {
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 4,
                0 => 5,
                _ => 6,
            };
            let pb = match b.kind {
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 4,
                0 => 5,
                _ => 6,
            };
            a.tick.cmp(&b.tick).then(pa.cmp(&pb))
        });

        Self { events, cursor: 0 }
    }

    pub fn total_events(&self) -> usize {
        self.events.len()
    }

    /// 获取下一个事件
    pub fn next_event(&mut self) -> Option<MergedEvent> {
        let ev = *self.events.get(self.cursor)?;
        self.cursor += 1;
        Some(ev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_model::{NoteEvent, track::TrackManager};

    fn make_doc(notes: Vec<Vec<NoteEvent>>, total_ticks: u32) -> MidiDocument {
        let track_count = notes.len() as u16;
        MidiDocument {
            next_note_id: 1,
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

            track_max_end_ticks: vec![],
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

    #[test]
    fn test_overlapping_notes_same_track() {
        // 同轨重叠：A(0,10) B(5,15) — 旧游标实现会把 B On 推迟到 10，此测试回归该 bug
        let doc = make_doc(
            vec![vec![
                NoteEvent::new(0, 10, 60, 100, 0),
                NoteEvent::new(5, 15, 64, 100, 0),
            ]],
            15,
        );
        let mut stream = MidiDocEventStream::new(&doc);
        assert_eq!(stream.total_events(), 4);
        let e1 = stream.next_event().unwrap();
        assert_eq!((e1.tick, e1.kind, e1.param1), (0, 0, 60));
        let e2 = stream.next_event().unwrap();
        assert_eq!((e2.tick, e2.kind, e2.param1), (5, 0, 64), "B On 应在 5 而非 10");
        let e3 = stream.next_event().unwrap();
        assert_eq!((e3.tick, e3.kind, e3.param1), (10, 1, 60));
        let e4 = stream.next_event().unwrap();
        assert_eq!((e4.tick, e4.kind, e4.param1), (15, 1, 64));
        assert!(stream.next_event().is_none());
    }

    #[test]
    fn test_overlapping_notes_same_tick_priority() {
        // 同 tick：NoteOff 优先于 NoteOn
        let doc = make_doc(
            vec![vec![
                NoteEvent::new(0, 10, 60, 100, 0),
                NoteEvent::new(10, 20, 62, 100, 0),
            ]],
            20,
        );
        let mut stream = MidiDocEventStream::new(&doc);
        // tick 10 同时有 A Off 和 B On，Off 应先
        let mut events = Vec::new();
        while let Some(e) = stream.next_event() {
            events.push((e.tick, e.kind));
        }
        assert_eq!(events, vec![(0, 0), (10, 1), (10, 0), (20, 1)]);
    }
}
