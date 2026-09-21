//! 键盘颜色清除行为：停止后清空与幂等

use super::*;

#[test]
fn test_keyboard_colors_clear_after_stop() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(doc);

    // 播放到 tick 240（C4、G4 着色）
    editor.playback_position = 240.0;
    editor.update_playback_key_colors();
    assert_ne!(
        editor.playback_key_colors[60 * 4 + 3],
        0,
        "C4 should be colored before stop"
    );
    assert_ne!(
        editor.playback_key_colors[67 * 4 + 3],
        0,
        "G4 should be colored before stop"
    );

    // 模拟「停止播放」：位置不一定归零（自然结束停在 tick 240），
    // 但键盘颜色必须恢复为无颜色状态。
    editor.playback_position = 240.0;
    editor.clear_playback_key_colors();

    assert_eq!(
        editor.playback_key_colors, [0u8; 1024],
        "键盘颜色应在停止后恢复无颜色状态"
    );
    // 扫描状态也应被重置，下次播放从头扫描
    assert_eq!(
        editor.playback_scan_state.last_tick, 0.0,
        "扫描状态应在停止后重置"
    );
    // 不应改动播放指示线位置
    assert_eq!(
        editor.playback_position, 240.0,
        "clear 不应改写 playback_position"
    );
}

#[test]
fn test_keyboard_colors_clear_is_idempotent() {
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    // 无文档、无颜色：clear 不应 panic 也不应产生颜色
    editor.clear_playback_key_colors();
    assert_eq!(editor.playback_key_colors, [0u8; 1024]);
}
