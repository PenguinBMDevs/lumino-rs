//! 键盘颜色功能测试
//!
//! 验证 `update_playback_key_colors` 能否从 MIDI 文档正确读取音符并着色。

use crate::Editor;
use lumino_midi_loader::{MidiDocument, NoteEvent};

/// 创建一个简单的 MIDI 文档用于测试
fn make_test_doc() -> MidiDocument {
    // 音轨 0：2 个音符（排序前检查排序稳定性）
    let mut track0 = vec![
        NoteEvent::new(480, 960, 64, 100, 0), // E4, 从 tick 480 到 960
        NoteEvent::new(0, 480, 60, 100, 0),   // C4, 从 tick 0 到 480
    ];
    track0.sort_unstable_by_key(|n| n.start_tick);

    // 音轨 1：1 个长音符
    let track1 = vec![
        NoteEvent::new(0, 1920, 67, 100, 1), // G4, 从 tick 0 到 1920
    ];

    let track_count = 2u16;
    MidiDocument {
        notes: vec![track0, track1],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: Vec::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Track 1".into()), Some("Track 2".into())],
        total_ticks: 1920,
        track_count,
        tracks: lumino_midi_loader::TrackManager::new(track_count),
        division: 480,
    }
}

#[test]
fn test_keyboard_colors_binary_search_at_mid_note() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(std::sync::Arc::new(doc));

    // 设置播放位置为 tick 240（在第一个音符中间）
    editor.playback_position = 240.0;
    editor.update_playback_key_colors();

    // 验证：C4 (key=60) 和 G4 (key=67) 应该被着色
    assert_ne!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should be colored at tick 240"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should be colored at tick 240"
    );
    // E4 (key=64) 从 tick 480 开始，在 tick 240 不应该被着色
    assert_eq!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should NOT be colored at tick 240"
    );
}

#[test]
fn test_keyboard_colors_at_note_boundary() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(std::sync::Arc::new(doc));

    // 在 tick 480：C4 刚结束 (end=480)，E4 刚开 始 (start=480)
    // 活动音符判断：start <= 480 < end
    // C4: 0 <= 480 < 480 → false（已结束）
    // E4: 480 <= 480 < 960 → true（刚开始）
    editor.playback_position = 480.0;
    editor.update_playback_key_colors();

    assert_eq!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should have ended at tick 480"
    );
    assert_ne!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should be active at tick 480"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should still be active at tick 480"
    );
}

#[test]
fn test_keyboard_colors_with_disable() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = false; // 功能关闭
    editor.editor_state.data.document = Some(std::sync::Arc::new(doc));

    editor.playback_position = 240.0;
    editor.update_playback_key_colors();

    // 功能关闭时不生成颜色
    assert_eq!(editor.playback_key_colors, [0u8; 1024]);
}

#[test]
fn test_keyboard_colors_no_document() {
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    // document 为 None

    editor.playback_position = 240.0;
    editor.update_playback_key_colors();

    // 无文档时不生成颜色（不应该 crash）
    assert_eq!(editor.playback_key_colors, [0u8; 1024]);
}

#[test]
fn test_keyboard_colors_tick_0() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(std::sync::Arc::new(doc));

    // 播放位置为 0（停止状态）
    editor.playback_position = 0.0;
    editor.update_playback_key_colors();

    // 停止时清空颜色
    assert_eq!(editor.playback_key_colors, [0u8; 1024]);
}

#[test]
fn test_keyboard_colors_tick_after_all_notes_end() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(std::sync::Arc::new(doc));

    // tick 2000：所有音符已结束（最大 end=1920）
    editor.playback_position = 2000.0;
    editor.update_playback_key_colors();

    // 所有音符应结束，没有颜色
    assert_eq!(editor.playback_key_colors, [0u8; 1024]);
}

/// 验证增量扫描路径：连续调用 `update_playback_key_colors` 时，
/// 第二次以后走增量路径（而非全量重建），结果必须与单次调用一致。
///
/// 此测试覆盖修复 BUG（线性增长）所引入的新代码路径：
/// 1. 首次调用 → 全量重建（doc_addr 缓存）
/// 2. tick 前进 → 增量扫描新进入活跃的音符
/// 3. retain 清理已结束音符
/// 4. tick 回退 → 触发全量重建
#[test]
fn test_keyboard_colors_incremental_scan() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(std::sync::Arc::new(doc));

    // === 阶段 1：tick=240，首次调用 → 全量重建路径 ===
    // C4(0..480) 和 G4(0..1920) 活跃，E4(480..960) 未开始
    editor.playback_position = 240.0;
    editor.update_playback_key_colors();
    assert_ne!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should be colored at tick 240 (full rebuild)"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should be colored at tick 240 (full rebuild)"
    );
    assert_eq!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should NOT be colored at tick 240"
    );

    // === 阶段 2：tick=480，tick 前进 → 增量扫描路径 ===
    // C4 刚结束（end=480），E4 刚开始（start=480），G4 仍活跃
    // 关键：增量路径必须正确把 C4 从 active_notes 中清理掉
    editor.playback_position = 480.0;
    editor.update_playback_key_colors();
    assert_eq!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should be cleaned by retain at tick 480 (incremental)"
    );
    assert_ne!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should be added by incremental scan at tick 480"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should remain active at tick 480"
    );

    // === 阶段 3：tick=960，继续前进 → 增量清理 E4 ===
    // E4(480..960) 刚结束，G4(0..1920) 仍活跃
    editor.playback_position = 960.0;
    editor.update_playback_key_colors();
    assert_eq!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should be cleaned by retain at tick 960"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should still be active at tick 960"
    );

    // === 阶段 4：tick=300，tick 回退 → 触发全量重建 ===
    // 此时 active_notes 已被全量重建覆盖，C4 G4 应再次活跃
    editor.playback_position = 300.0;
    editor.update_playback_key_colors();
    assert_ne!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should be active again after seek backward (full rebuild)"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should still be active after seek backward"
    );
    assert_eq!(
        editor.playback_key_colors[64 * 4 + 3],
        0,
        "E4 should not be active at tick 300"
    );
}

/// 验证活跃音符集合不会无限增长——模拟长时间播放（连续帧 tick 前进），
/// 确认 retain 正确清理已结束音符，active_notes 长度有上限。
#[test]
fn test_keyboard_colors_incremental_no_growth() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(std::sync::Arc::new(doc));

    // 从 tick=0 开始连续推进到 tick=2000，每步 10 tick
    let mut last_active_len = 0usize;
    for tick in (0..=2000u32).step_by(10) {
        editor.playback_position = tick as f32;
        editor.update_playback_key_colors();
        let active_len = editor.playback_scan_state.active_notes.len();
        // 活跃音符集合不会随 tick 累积——测试文档只有 3 个音符，活跃数应 ≤ 3
        assert!(
            active_len <= 3,
            "active_notes should not grow unbounded: tick={} len={}",
            tick,
            active_len
        );
        last_active_len = active_len;
    }
    // tick=2000 时所有音符已结束，active_notes 应为空
    assert_eq!(last_active_len, 0, "active_notes should be empty at end");
}
