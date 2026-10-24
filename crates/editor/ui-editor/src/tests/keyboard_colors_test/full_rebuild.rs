//! 键盘颜色全量重扫路径：跨轨重扫不 panic 且结果正确
//!
//! 2026-10-24：原先本文件覆盖的是「跨轨空间索引 `update_query` 全量重建」路径。
//! 该索引因构建成本 O(N log N)（1600W 音符 2-4s）已从渲染/命中路径退役，
//! 键色成为最后一个消费者后一并退役，全量重扫改回 `ChunkedList::iter_window`
//! 窗口扫描。本文件保留跨轨正确性覆盖。

use super::*;

/// 验证跨所有轨的全量重扫路径：较大文档下重扫须与逐音符扫描一致，
/// 且键位 → 颜色偏移映射无越界 panic。
#[test]
fn test_keyboard_colors_full_rescan_across_tracks() {
    // 2 轨，每轨 200 音符（合计 400）
    let mut track0 = Vec::new();
    let mut track1 = Vec::new();
    for i in 0..200u32 {
        // tick 0,40,80,... 长度 40，键位 60..79 / 72..91 循环
        track0.push(NoteEvent::new(
            i * 40,
            i * 40 + 40,
            60u8 + (i % 20) as u8,
            100,
            0,
        ));
        track1.push(NoteEvent::new(
            i * 40,
            i * 40 + 40,
            72u8 + (i % 20) as u8,
            100,
            1,
        ));
    }
    track0.sort_unstable_by_key(|n| n.start_tick);
    track1.sort_unstable_by_key(|n| n.start_tick);

    let doc = MidiDocument {
        notes: vec![
            lumino_midi_loader::ChunkedList::from_sorted(track0),
            lumino_midi_loader::ChunkedList::from_sorted(track1),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T0".into()), Some("T1".into())],
        total_ticks: 8000,
        track_count: 2u16,
        tracks: lumino_midi_loader::TrackManager::new(2),
        division: 480,
        track_ports: vec![],

        track_max_end_ticks: vec![],
        track_max_note_lens: vec![],
    };

    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(doc);

    // tick=100：每轨仅 start<=100<end 的音符活跃 → i=2 (start=80,end=120)
    // track0: key=60+2=62；track1: key=72+2=74。其余键不应着色。
    editor.playback_position = 100.0;
    editor.update_playback_key_colors();
    assert_ne!(
        editor.playback_key_colors[62 * 4 + 3],
        0,
        "key 62 应被着色 (track0)"
    );
    assert_ne!(
        editor.playback_key_colors[74 * 4 + 3],
        0,
        "key 74 应被着色 (track1)"
    );
    assert_eq!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "key 60 不应着色（i=0 已结束）"
    );

    // 继续推进到 tick=9000（所有音符已结束），跨轨重扫不应 panic 且颜色清空
    editor.playback_position = 9000.0;
    editor.update_playback_key_colors();
    assert_eq!(editor.playback_key_colors, [0u8; 1024]);
}
