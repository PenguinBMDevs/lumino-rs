//! Runner 文件菜单：素材工程构建（.lmmaterial）
//!
//! 从选区音符直接构建素材工程，与 editor_midi.rs 分离，
//! 保持模块职责单一且文件长度合规。

use lumino_midi_loader::MidiDocument;

use super::editor_midi::EditorNotes;

/// 从选区音符**直接**构建素材工程（.lmmaterial 专用，不走 MIDI 字节中转）
///
/// 设计动机（修复音符长度错乱）：
/// - MIDI 中转（字节编码 → 回读）存在 f32→u32 截断与 midly NoteOn/NoteOff
///   配对语义差异，同 key 相接/重叠音符回读后长度错乱（部分变短部分变长）；
/// - 本函数直接以整数 tick 构建 `CompactEvent`（精确 round），配合稳定排序
///   （同 tick NoteOff 在 NoteOn 前）与回读端 FIFO 配对，长度无损。
///
/// 时间轴归一化（修复素材范围错位）：
/// - **开头对齐第一个音符**：所有事件（音符/控制/全局数据）平移，使素材
///   第一个音符从 tick 0 开始，片段前的空白不进入素材；
/// - **末尾为最后一个音符**：`total_ticks` = 最后一个音符结束 tick - 第一个
///   音符开始 tick，素材长度由音符内容决定，**不跟随用户框选范围**；
/// - 片段前的控制事件（CC/PC/弯音）、拍号/调号/歌词/SysEx/标记丢弃；
///   片段前的 tempo 平移后收敛到 0（素材必须保留速度信息）。
///
/// - 音符：仅选中音符（跨轨）；
/// - 控制事件（CC/PC/弯音）：仅选中轨道；
/// - 全局数据（tempo/拍号/调号/歌词/SysEx）：全量保留（含时间轴平移）。
pub(super) fn build_material_project_from_selection(
    doc: &MidiDocument,
    selected: &EditorNotes,
) -> lumino_export::LuminoProject {
    use lumino_midi_loader::{CompactEvent, EventKind};
    use lumino_project::{LmtrackData, TrackMeta, TrackVisibilitySer};

    let mut project = lumino_export::LuminoProject::new("Material");
    project.metadata.audio.division = doc.division;

    // ── 素材时间范围：第一个音符开始 → 最后一个音符结束 ──
    let mut min_start = f32::INFINITY;
    let mut max_end = f32::NEG_INFINITY;
    for (_, notes) in selected {
        for &(tick, _, length, _, _) in notes {
            let start = tick.round();
            let end = (tick + length).round().max(start + 1.0);
            min_start = min_start.min(start);
            max_end = max_end.max(end);
        }
    }
    let offset = min_start as u32;
    let total_ticks = (max_end as u32).saturating_sub(offset).max(1);
    project.metadata.audio.total_ticks = total_ticks;

    // 全局数据：整体平移时间轴（片段前的拍号/调号/歌词/SysEx/标记丢弃，
    // 片段前的 tempo 收敛到 0 保留）
    project.tempo_changes = doc
        .tempo_changes
        .iter()
        .map(|&(tick, bpm)| (tick.saturating_sub(offset), bpm))
        .collect();
    project.time_signatures = doc
        .time_signatures
        .iter()
        .filter(|&&(tick, _, _)| tick >= offset)
        .map(|&(tick, num, den)| (tick - offset, num, den))
        .collect();
    project.key_signatures = doc
        .key_signatures
        .iter()
        .filter(|&&(tick, _, _)| tick >= offset)
        .map(|&(tick, sharps, minor)| (tick - offset, sharps, minor))
        .collect();
    project.lyrics = doc
        .lyrics
        .iter()
        .filter(|e| e.0 >= offset)
        .map(|e| (e.0 - offset, e.1, e.2.clone()))
        .collect();
    project.markers = doc
        .markers
        .iter()
        .filter(|e| e.0 >= offset)
        .map(|e| (e.0 - offset, e.1, e.2.clone()))
        .collect();
    project.sys_ex = doc
        .sys_ex
        .iter()
        .filter(|e| e.0 >= offset)
        .map(|e| (e.0 - offset, e.1, e.2.clone()))
        .collect();
    project.track_names = doc.track_names.clone();

    // 选中轨道集合（控制事件过滤基准）
    let selected_tracks: std::collections::HashSet<u16> =
        selected.iter().map(|(t, _)| *t as u16).collect();

    // 控制事件：仅保留选中轨道、且位于素材时间范围内的自动化数据
    for ev in &doc.control_events {
        let ev_track = ev.track; // packed 字段先拷贝，避免未对齐引用
        if !selected_tracks.contains(&ev_track) || ev.tick < offset {
            continue;
        }
        let new_tick = ev.tick - offset;
        match ev.kind {
            0 => {
                let (controller, value) = ev.as_control_change();
                project
                    .control_changes
                    .push((new_tick, ev.track, ev.channel, controller, value));
            }
            1 => {
                let program = ev.as_program_change();
                project
                    .program_changes
                    .push((new_tick, ev.track, ev.channel, program));
            }
            2 => {
                let normalized = ev.as_pitch_bend();
                let bend_offset = (normalized * 8192.0).round() as i16;
                project
                    .pitch_bends
                    .push((new_tick, ev.track, ev.channel, bend_offset));
            }
            _ => {}
        }
    }

    // 每轨构建 CompactEvent（精确 round，无 f32→u32 截断）
    let mut total_notes: u64 = 0;
    for (track_id, notes) in selected {
        let mut track_events: Vec<CompactEvent> = Vec::with_capacity(notes.len() * 2);
        for &(tick, key, length, velocity, channel) in notes {
            // 时间轴归一化：相对第一个音符平移，开头对齐 tick 0
            let start = (tick.round() as i64 - offset as i64).max(0) as u32;
            let end = ((tick + length).round() as i64 - offset as i64).max(start as i64 + 1) as u32;
            track_events.push(CompactEvent::new(
                start,
                *track_id as u16,
                EventKind::NoteOn,
                channel,
                key as u16,
                velocity as u16,
            ));
            track_events.push(CompactEvent::new(
                end,
                *track_id as u16,
                EventKind::NoteOff,
                channel,
                key as u16,
                velocity as u16,
            ));
        }
        // 稳定排序（保持声明顺序：同 tick 的 NoteOff 先于后续音符的 NoteOn）
        track_events.sort_by_key(|e| e.delta_tick());

        // 绝对 tick → 相对 delta_tick
        let mut last_tick = 0_u32;
        for ev in &mut track_events {
            let abs_tick = ev.delta_tick();
            ev.set_delta_tick(abs_tick.saturating_sub(last_tick));
            last_tick = abs_tick;
        }

        let channel = track_events
            .iter()
            .find(|ev| ev.kind().is_note())
            .map(|ev| ev.channel())
            .unwrap_or(0);
        let max_tick = track_events
            .iter()
            .scan(0_u32, |acc, ev| {
                *acc = acc.saturating_add(ev.delta_tick());
                Some(*acc)
            })
            .last()
            .unwrap_or(0);
        let name = doc.track_name(*track_id).unwrap_or("").to_string();

        let meta = TrackMeta {
            track_id: *track_id as u16,
            name,
            channel,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: channel == 9,
            max_tick,
        };
        let track_data = LmtrackData::from_compact_events(meta, &track_events);
        project.add_track(track_data);
        total_notes += notes.len() as u64;
    }

    project.metadata.audio.total_notes = total_notes;
    project.metadata.audio.track_count = selected.len() as u16;
    project
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::menu::file::editor_midi::EditorNotes;

    /// 构造带"引子空白"的文档：第一个音符在 tick 1000（素材不应包含引子空白）
    fn make_doc_with_leading_gap() -> MidiDocument {
        use lumino_midi_loader::{ChunkedList, NoteEvent, TrackManager};

        MidiDocument {
            notes: vec![ChunkedList::from_sorted(vec![
                NoteEvent::new(1000, 1480, 60, 100, 0),
                NoteEvent::new(2000, 2240, 62, 90, 0),
            ])],
            tempo_changes: vec![(0, 90.0)],
            time_signatures: vec![(0, 2, 4), (1000, 4, 4)],
            key_signatures: vec![],
            control_events: ChunkedList::from_sorted(vec![
                midly::loader::PackedControlEvent::control_change(500, 0, 0, 10, 64),
                midly::loader::PackedControlEvent::control_change(1200, 0, 0, 10, 80),
            ]),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![Some("Piano".to_string())],
            total_ticks: 3000,
            track_count: 1,
            tracks: TrackManager::new(1),
            division: 480,
            track_ports: vec![0],
            track_max_end_ticks: MidiDocument::new_track_max_ticks(1),
        }
    }

    /// 素材时间轴归一化：开头对齐第一个音符，长度由最后一个音符决定
    #[test]
    fn test_build_material_normalizes_to_first_note() {
        use lumino_midi_loader::{CompactEvent, EventKind};

        let doc = make_doc_with_leading_gap();
        // 框选仅覆盖音符本身（与走带框选相同语义：只取选中音符）
        let selected: EditorNotes = vec![(
            0,
            vec![(1000.0, 60, 480.0, 100, 0), (2000.0, 62, 240.0, 90, 0)],
        )];

        let project = build_material_project_from_selection(&doc, &selected);

        // 素材长度 = 最后一个音符结束(2240) - 第一个音符开始(1000)，不跟随框选/引子空白
        assert_eq!(project.metadata.audio.total_ticks, 1240);
        assert_eq!(project.metadata.audio.total_notes, 2);
        // 片段前的拍号（2/4 引子）丢弃，片段内拍号平移到 0
        assert_eq!(project.time_signatures, vec![(0, 4, 4)]);
        // 片段前的 tempo 收敛到 0 保留（速度信息不可丢）
        assert_eq!(project.tempo_changes, vec![(0, 90.0)]);
        // 片段前的 CC 丢弃，片段内的平移到素材时间轴
        assert_eq!(project.control_changes, vec![(200, 0, 0, 10, 80)]);

        // 音符事件：第一个音符从 0 开始，第二个音符结束于 total_ticks
        let track = project.get_track(0).expect("应包含音轨 0");
        let events: Vec<CompactEvent> = track.compact_events().expect("解码音轨失败").collect();
        let mut tick = 0_u32;
        let mut note_starts = Vec::new();
        let mut note_ends = Vec::new();
        for ev in events {
            tick = tick.saturating_add(ev.delta_tick());
            if ev.kind() == EventKind::NoteOn {
                note_starts.push(tick);
            } else if ev.kind() == EventKind::NoteOff {
                note_ends.push(tick);
            }
        }
        assert_eq!(note_starts, vec![0, 1000]);
        assert_eq!(note_ends, vec![480, 1240]);
    }

    /// 素材开头无空白（第一个音符在 tick 0）时保持原样
    #[test]
    fn test_build_material_keeps_original_when_start_at_zero() {
        use lumino_midi_loader::{ChunkedList, NoteEvent, TrackManager};

        let doc = MidiDocument {
            notes: vec![ChunkedList::from_sorted(vec![NoteEvent::new(
                0, 480, 60, 100, 0,
            )])],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![],
            control_events: lumino_midi_loader::ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![None],
            total_ticks: 480,
            track_count: 1,
            tracks: TrackManager::new(1),
            division: 480,
            track_ports: vec![0],
            track_max_end_ticks: MidiDocument::new_track_max_ticks(1),
        };
        let selected: EditorNotes = vec![(0, vec![(0.0, 60, 480.0, 100, 0)])];

        let project = build_material_project_from_selection(&doc, &selected);
        assert_eq!(project.metadata.audio.total_ticks, 480);
        assert_eq!(project.time_signatures, vec![(0, 4, 4)]);
        assert_eq!(project.tempo_changes, vec![(0, 120.0)]);
    }
}
