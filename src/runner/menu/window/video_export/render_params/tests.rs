use super::*;
use lumino_midi_loader::{NoteEvent, TrackManager};

fn make_track(notes: &[(u32, u32, u8)]) -> Vec<NoteEvent> {
    let mut v: Vec<NoteEvent> = notes
        .iter()
        .map(|&(s, e, k)| NoteEvent::new(s, e, k, 100, 0))
        .collect();
    v.sort_unstable_by_key(|n| n.start_tick);
    v
}

/// 正确性护栏：下界固定为 0 后，窗口从文件头开始，但上界仍通过二分查找
/// 限制在 `tick_end` 以内，不会退化为全量扫描。
#[test]
fn test_note_search_bounds_window_is_small() {
    // 100 万音符均匀分布在 [0, 10_000_000) tick
    const TOTAL: usize = 1_000_000;
    let mut track = Vec::with_capacity(TOTAL);
    for i in 0..TOTAL {
        let t = (i as u32) * 10;
        track.push(NoteEvent::new(t, t + 240, 60, 100, 0));
    }

    // 视口：tick 5_000_000 起，窗口 4 小节（ppq=480 → 7680 ticks）
    let chunked = lumino_midi_loader::ChunkedList::from_sorted(track);
    let (start, end) = note_search_bounds(&chunked, 5_000_000, 5_007_680);
    let window_len = end - start;

    // 下界为 0，窗口从文件头开始
    assert_eq!(start, 0, "下界应固定为 0");
    // 上界仍通过二分查找限制在 tick_end 以内，不会扫描文件末尾
    assert!(window_len < TOTAL, "窗口不应覆盖全部音符");
    assert!(window_len > 0, "窗口不应为空");
    // 窗口应包含所有 start_tick <= tick_end 的音符
    assert!(chunked.get(end - 1).expect("窗口内应有音符").start_tick <= 5_007_680);
    if end < TOTAL {
        assert!(
            chunked
                .get(end)
                .expect("end < TOTAL 时 end 处应有音符")
                .start_tick
                > 5_007_680
        );
    }
}

/// 正确性：二分窗口收集结果必须与全量遍历完全一致
/// （覆盖：视口前已结束、跨视口长音符、视口内、视口后未开始）
#[test]
fn test_visible_notes_collection_matches_full_scan() {
    let doc = MidiDocument {
        next_note_id: 1,
        notes: vec![
            lumino_midi_loader::ChunkedList::from_sorted(make_track(&[
                (0, 480, 40),               // 视口前很远，已结束
                (4_985_000, 5_001_000, 50), // 跨视口长音符（时长 16000 < BUFFER）
                (5_000_100, 5_001_000, 60), // 视口内
                (5_007_000, 5_009_000, 62), // 跨视口右边界
                (5_007_680, 5_008_000, 64), // 视口上界恰好开始
                (6_000_000, 6_000_480, 70), // 视口后很远，未开始
            ])),
            lumino_midi_loader::ChunkedList::from_sorted(make_track(&[(5_000_200, 5_000_700, 65)])),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T1".into()), Some("T2".into())],
        total_ticks: 6_000_480,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    };

    let tick_start = 5_000_000;
    let tick_end = tick_start + 7680;
    const KEY_COUNT: u16 = 128;

    // 窗口版（被测窗口语义：上界二分 + 可见过滤）
    let mut windowed = Vec::new();
    for (track_idx, track_notes) in doc.notes.iter().enumerate() {
        let (_, search_end) = note_search_bounds(track_notes, tick_start, tick_end);
        for n in track_notes.iter().take(search_end) {
            if n.end_tick > tick_start && n.start_tick < tick_end && n.key < KEY_COUNT as u8 {
                windowed.push((n.key, n.start_tick, n.end_tick, track_idx as u16));
            }
        }
    }

    // 全量遍历版（参考实现）
    let mut full = Vec::new();
    for (track_idx, track_notes) in doc.notes.iter().enumerate() {
        for n in track_notes {
            if n.end_tick > tick_start && n.start_tick < tick_end && n.key < KEY_COUNT as u8 {
                full.push((n.key, n.start_tick, n.end_tick, track_idx as u16));
            }
        }
    }

    assert_eq!(windowed, full, "二分窗口收集结果与全量遍历不一致");
    // 预期可见：跨视口长音符 + 视口内 2 个 + 跨右边界 1 个
    assert_eq!(windowed.len(), 4);
}

/// 正确性：滑动窗口收集在 tick 单调推进（含超长跨视口音符、回退重置）下，
/// 每一帧输出必须与旧"逐帧全前缀扫描"逐元素一致。
#[test]
fn test_collect_window_notes_matches_prefix_scan() {
    // T0：超长音符横跨整个导出（0..10M）+ 密集短音符；T1：稀疏音符 + 越界 key。
    let mut t0: Vec<(u32, u32, u8)> = vec![(0, 10_000_000, 60)];
    for i in 0..2000 {
        let s = i * 5000;
        t0.push((s, s + 240, (i % 128) as u8));
    }
    let doc = MidiDocument {
        next_note_id: 1,
        notes: vec![
            lumino_midi_loader::ChunkedList::from_sorted(make_track(&t0)),
            lumino_midi_loader::ChunkedList::from_sorted(make_track(&[
                (100, 200, 10),
                (4_000_000, 4_000_500, 200), // 越界 key，必须被过滤
                (9_999_000, 9_999_480, 70),
            ])),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T1".into()), Some("T2".into())],
        total_ticks: 10_000_000,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    };
    const KEY_COUNT: u16 = 128;
    const SPAN: u32 = 7680;

    // 旧实现（参考）：每帧从 0 扫描到 search_end。
    fn reference(doc: &MidiDocument, tick_start: u32, tick_end: u32) -> Vec<(u8, u32, u32, u16)> {
        let mut out = Vec::new();
        for (track_idx, track_notes) in doc.notes.iter().enumerate() {
            let (_, search_end) = note_search_bounds(track_notes, tick_start, tick_end);
            for n in track_notes.iter().take(search_end) {
                if n.end_tick > tick_start && n.start_tick < tick_end && n.key < KEY_COUNT as u8 {
                    out.push((n.key, n.start_tick, n.end_tick, track_idx as u16));
                }
            }
        }
        out
    }

    let mut state = WindowCollectState::default();
    let mut got = Vec::new();
    // 单调推进 20 帧（tick ≤ 9.5M < 长音符 end=10M）：超长音符必须全程在场，游标只进不退。
    let mut tick = 0u32;
    for _ in 0..20 {
        let tick_end = tick.saturating_add(SPAN);
        collect_window_notes(&doc, tick, tick_end, KEY_COUNT, &mut state, &mut got);
        let expected = reference(&doc, tick, tick_end);
        let got_tuples: Vec<(u8, u32, u32, u16)> = got
            .iter()
            .map(|n| (n.key, n.start_tick, n.start_tick + n.length, n.track_idx))
            .collect();
        assert_eq!(got_tuples, expected, "tick={tick} 时滑动收集与旧扫描不一致");
        // 超长音符 (key=60, start=0) 必须全程可见。
        assert!(
            got_tuples.iter().any(|&(k, s, _, _)| k == 60 && s == 0),
            "tick={tick} 时超长音符丢失"
        );
        tick += 500_000;
    }
    // 回退重置：tick 跳回起点，输出仍须一致（游标清零重建）。
    collect_window_notes(&doc, 0, SPAN, KEY_COUNT, &mut state, &mut got);
    let got_tuples: Vec<(u8, u32, u32, u16)> = got
        .iter()
        .map(|n| (n.key, n.start_tick, n.start_tick + n.length, n.track_idx))
        .collect();
    assert_eq!(
        got_tuples,
        reference(&doc, 0, SPAN),
        "tick 回退后输出不一致"
    );
}
#[test]
fn test_resolve_miditrail_speed_isolated_per_view() {
    assert_eq!(
        resolve_miditrail_speed(MiditrailViewMode::Normal, 2.0, 5.0),
        2.0,
        "Normal 应取 Normal 速度"
    );
    assert_eq!(
        resolve_miditrail_speed(MiditrailViewMode::Top, 2.0, 5.0),
        5.0,
        "Top 应取 Top 速度"
    );
    // 下限钳制（与旧 waterfall_scroll_speed.max(0.1) 语义一致）。
    assert_eq!(
        resolve_miditrail_speed(MiditrailViewMode::Top, 1.0, 0.0),
        0.1
    );
}

/// 视图枚举解析：UI 字符串 → 强类型（非法值回退 Normal，不静默产生第三种状态）。
#[test]
fn test_miditrail_view_mode_from_str() {
    use std::str::FromStr;
    assert_eq!(
        MiditrailViewMode::from_str("Top").expect("Top 应可解析"),
        MiditrailViewMode::Top
    );
    assert_eq!(
        MiditrailViewMode::from_str("普通").expect("普通应可解析"),
        MiditrailViewMode::Normal
    );
    assert!(MiditrailViewMode::from_str("侧视").is_err());
}
