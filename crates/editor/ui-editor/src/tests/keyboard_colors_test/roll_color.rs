//! 键盘键色与卷帘音符颜色的**统一性**守护
//!
//! 卷帘着色规则是两段式的（见 `lumino_extras::palette::note_track_color`）：
//! - 当前编辑轨（主音轨）= 固定蓝 `MAIN_TRACK_NOTE_COLOR`（4 个 shader 的
//!   `MAIN_TRACK_COLOR`）；
//! - 其余轨 = 调色板色 `current_track_color(doc_track)`。
//!
//! 键盘高亮必须**逐字节**照此取色，否则同一轨的键色与音符色会不一致
//! （2026-10-25 之前键盘对所有轨一律用调色板色，主音轨因此对不上）。

use super::*;
use lumino_extras::palette::{MAIN_TRACK_NOTE_COLOR, current_track_color};

/// 主音轨的键色 = 卷帘音符的固定蓝；其他轨的键色 = 该轨调色板色。
#[test]
fn test_keyboard_colors_match_roll_note_colors() {
    let doc = make_test_doc(); // track0: C4(60)/E4(64)；track1: G4(67)
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(doc);

    // 默认当前轨 = 0，即 track0 为主音轨
    assert_eq!(
        editor.editor_state.data.current_track, 0,
        "前置：默认当前轨应为 0"
    );

    editor.playback_position = 240.0;
    editor.update_playback_key_colors();

    // 主音轨（track0）的 C4 → 固定蓝（与 shader MAIN_TRACK_COLOR 一致）
    assert_eq!(
        &editor.playback_key_colors[60 * 4..60 * 4 + 4],
        &MAIN_TRACK_NOTE_COLOR,
        "主音轨键色必须等于卷帘音符的固定蓝"
    );
    // 其他轨（track1）的 G4 → 该轨调色板色
    assert_eq!(
        &editor.playback_key_colors[67 * 4..67 * 4 + 4],
        &current_track_color(1),
        "非主音轨键色必须等于该轨音符的调色板色"
    );
}

/// 切换当前轨后，同一音符的键色必须跟着卷帘换色（主音轨语义随 `current_track` 走）。
#[test]
fn test_keyboard_colors_follow_current_track() {
    let doc = make_test_doc();
    let mut editor = Editor::new();
    editor.playback_key_colors_enabled = true;
    editor.editor_state.data.document = Some(doc);

    editor.playback_position = 240.0;
    editor.update_playback_key_colors();
    assert_eq!(
        &editor.playback_key_colors[67 * 4..67 * 4 + 4],
        &current_track_color(1),
        "track1 非主音轨时取调色板色"
    );

    // 把当前轨切到 1 → track1 变主音轨，取固定蓝。
    //
    // 关键不变式：**切轨只需换取色策略，不需要重扫**（聚合里存的是音轨号，
    // 颜色在 emit 时解析）。因此这里既不动内容世代、也不动 tick，仅切
    // `current_track` 再调一次更新，键色就必须立刻变。
    editor.editor_state.data.current_track = 1;
    editor.update_playback_key_colors();

    assert_eq!(
        &editor.playback_key_colors[67 * 4..67 * 4 + 4],
        &MAIN_TRACK_NOTE_COLOR,
        "track1 变为主音轨后键色应改为固定蓝（切轨不得需要重扫）"
    );
    // track0 退出主音轨 → 该轨音符改取调色板色
    assert_eq!(
        &editor.playback_key_colors[60 * 4..60 * 4 + 4],
        &current_track_color(0),
        "track0 不再是主音轨，键色应改为调色板色"
    );
}
