//! 键盘颜色增量扫描：连续调用结果一致与活跃集合不增长

use super::*;

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
    editor.editor_state.data.document = Some(doc);

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
    editor.editor_state.data.document = Some(doc);

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

/// 索引失效必须按**音符内容世代**判定，而不是文档内存地址。
///
/// 2026-10-17（审计 §五）：旧实现只比对 `doc as *const MidiDocument` 地址，
/// 而就地增删音符**不改变地址** → 索引被判为仍然有效，此后任何全量重建
/// （seek / 循环回绕 / tick 回退）都会拿**陈旧索引**着色：
/// 已删除的音符仍然着色、新增音符不着色。
#[test]
fn test_keyboard_colors_index_follows_notes_generation() {
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(make_test_doc());

    // 建索引：tick=240 时 C4(60) 与 G4(67) 活跃
    editor.playback_position = 240.0;
    editor.update_playback_key_colors();
    assert_ne!(editor.playback_key_colors[60 * 4 + 3], 0, "C4 初始应着色");

    // 就地删除 C4（track 0 中 start_tick 最小者，即 index 0），并 bump 音符世代
    // ——所有直接改动音符的入口都必须 `mark_track_notes_changed`（marks.rs 契约）
    {
        let data = &mut editor.editor_state.data;
        let doc = data.document.as_mut().expect("文档应存在");
        assert!(doc.remove_note(0, 0).is_some(), "应删除 C4");
        data.mark_track_notes_changed();
    }

    // tick 回退 → 触发全量重建：必须用**新内容**重建索引
    editor.playback_position = 120.0;
    editor.update_playback_key_colors();
    assert_eq!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "已删除的 C4 不得再着色：索引必须随音符世代失效并重建"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 仍活跃，应保持着色"
    );
}

/// 装载新文档后（同一 tick、无 seek）也必须用**新内容**重建索引。
///
/// 审计 §五 第二条：旧实现按文档内存地址判失效，而旧文档 drop 后新文档
/// **可能分配到同一地址** → 两份文档被判为同一份，残留上一份的键色。
/// 世代方案下「装载 = 世代变化」是硬前提，故这里按装载契约显式 bump
/// （与 `Root::set_midi_document` 的行为一致）。
#[test]
fn test_keyboard_colors_index_follows_document_replacement() {
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(make_test_doc());

    editor.playback_position = 240.0;
    editor.update_playback_key_colors();
    assert_ne!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "旧文档 C4 应着色"
    );

    // 替换为「同 tick 只有 D4(62)」的文档，并按装载契约 bump 世代
    editor.editor_state.data.document = Some(make_single_note_doc(62));
    editor.editor_state.data.mark_track_notes_changed();

    // 同一 tick（不落入 tick 回退 / seek 分支）也必须触发索引重建
    editor.update_playback_key_colors();
    assert_eq!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "旧文档的 C4 不得残留：索引必须随世代重建"
    );
    assert_ne!(
        editor.playback_key_colors[62 * 4 + 3],
        0,
        "新文档的 D4 应着色"
    );
}
