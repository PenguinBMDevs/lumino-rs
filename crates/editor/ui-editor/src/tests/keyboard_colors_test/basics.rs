//! 键盘颜色基础路径：二分定位、音符边界、功能开关、无文档与边界 tick

use super::*;

#[test]
fn test_keyboard_colors_binary_search_at_mid_note() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(doc);

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
    editor.editor_state.data.document = Some(doc);

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
    editor.editor_state.data.document = Some(doc);

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
    editor.editor_state.data.document = Some(doc);

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
    editor.editor_state.data.document = Some(doc);

    // tick 2000：所有音符已结束（最大 end=1920）
    editor.playback_position = 2000.0;
    editor.update_playback_key_colors();

    // 所有音符应结束，没有颜色
    assert_eq!(editor.playback_key_colors, [0u8; 1024]);
}
