//! 批量拖动预览序列测试
//!
//! 覆盖 BUG 修复：批量拖动（`DraggingSelection` / `DraggingSelectionCopy`）过程中
//! key 范围上下移动时没有发声反馈。修复后：
//! - key 偏移变化 → 按选中音符的 **tick 时间顺序** + **当前 ghost key 位置**构建预览序列；
//! - 各音符的播放时刻按 **工程 BPM 与 PPQ** 换算（真实时序，非固定间隔琶音）；
//! - key 偏移不变（纯水平移动）→ 不重建序列；
//! - key 偏移回到 0 → 清空序列；
//! - 序列经 `take_audio_actions` 在各自 `play_at` 时刻逐个弹出。

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_core::AudioAction;
use lumino_editor_state::DragState;
use std::time::Duration;
use std::time::Instant;

/// 进入批量拖动状态：选中 `indices`，拖动起点 (tick 0, key 60)
fn setup_dragging_selection(editor: &mut Editor, indices: &[usize]) {
    let note_count = editor.editor_state.data.current_track_note_count();
    let drag = DragState::from_indices(indices.iter().copied(), note_count, 0, 60);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
}

/// 读取当前预览序列的 key 列表
fn sequence_keys(editor: &Editor) -> Vec<u8> {
    editor
        .editor_state
        .interaction
        .preview_sequence
        .iter()
        .map(|note| note.key)
        .collect()
}

/// 以鼠标位置 (tick, key) 驱动一次批量拖动状态变化计算
fn drag_to(editor: &mut Editor, tick: f32, key: u16) {
    editor.compute_state_changes(tick, key, tick);
}

#[test]
fn test_dragging_selection_key_move_builds_time_ordered_sequence() {
    let mut editor = Editor::new();
    // 故意乱序 tick：验证序列按 tick 时间顺序（而非音符索引顺序）排列
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(480.0, 60, 100.0),
            Note::new(0.0, 62, 100.0),
            Note::new(960.0, 64, 100.0),
        ],
    );
    setup_dragging_selection(&mut editor, &[0, 1, 2]);

    // 鼠标下移 3 个 key（key=63）→ delta_key=3，应触发预览序列
    drag_to(&mut editor, 100.0, 63);

    // 序列 = 按 tick 升序的 ghost key（原始 key + delta_key）：
    // tick 0（key 62→65）、tick 480（key 60→63）、tick 960（key 64→67）
    assert_eq!(sequence_keys(&editor), vec![65, 63, 67]);
}

#[test]
fn test_dragging_selection_sequence_timing_follows_bpm() {
    let mut editor = Editor::new();
    // 测试辅助固定：tempo 120 BPM、division 480 PPQ
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 100.0),
            Note::new(480.0, 62, 100.0),  // 相隔 1 个四分音符
            Note::new(1920.0, 64, 100.0), // 相隔 4 个四分音符
        ],
    );
    setup_dragging_selection(&mut editor, &[0, 1, 2]);
    drag_to(&mut editor, 0.0, 63);

    let seq = &editor.editor_state.interaction.preview_sequence;
    assert_eq!(seq.len(), 3);
    // 120 BPM：1 个四分音符（480 tick） = 500ms；4 个四分音符（1920 tick） = 2000ms
    let d1 = seq[1].play_at.duration_since(seq[0].play_at);
    let d2 = seq[2].play_at.duration_since(seq[0].play_at);
    assert_eq!(d1, Duration::from_millis(500), "四分音符间隔应按 120 BPM 换算为 500ms");
    assert_eq!(d2, Duration::from_millis(2000), "4 个四分音符应为 2000ms");
}

#[test]
fn test_dragging_selection_sequence_timing_follows_custom_bpm() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(480.0, 62, 100.0)],
    );
    // 工程 tempo 改为 240 BPM：同一 tick 间隔的时间应减半
    editor
        .editor_state
        .data
        .document
        .as_mut()
        .expect("document 应存在")
        .tempo_changes = vec![(0, 240.0)];
    setup_dragging_selection(&mut editor, &[0, 1]);
    drag_to(&mut editor, 0.0, 63);

    let seq = &editor.editor_state.interaction.preview_sequence;
    assert_eq!(seq.len(), 2);
    let d = seq[1].play_at.duration_since(seq[0].play_at);
    assert_eq!(
        d,
        Duration::from_millis(250),
        "240 BPM 下四分音符应为 250ms"
    );
}

#[test]
fn test_dragging_selection_sequence_timing_uses_tempo_at_first_note() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 100.0),
            Note::new(960.0, 62, 100.0),
        ],
    );
    // 首个音符 tick=0 处 tempo 120，tick 480 后变 240：序列应取首个音符处的 tempo
    editor
        .editor_state
        .data
        .document
        .as_mut()
        .expect("document 应存在")
        .tempo_changes = vec![(0, 120.0), (480, 240.0)];
    setup_dragging_selection(&mut editor, &[0, 1]);
    drag_to(&mut editor, 0.0, 63);

    let seq = &editor.editor_state.interaction.preview_sequence;
    let d = seq[1].play_at.duration_since(seq[0].play_at);
    assert_eq!(
        d,
        Duration::from_millis(1000),
        "应使用首个音符 tick 处生效的 tempo（120 BPM）"
    );
}

#[test]
fn test_dragging_selection_key_change_replaces_sequence() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(480.0, 62, 100.0)],
    );
    setup_dragging_selection(&mut editor, &[0, 1]);

    drag_to(&mut editor, 0.0, 63); // delta_key=3 → [63, 65]
    assert_eq!(sequence_keys(&editor), vec![63, 65]);

    drag_to(&mut editor, 0.0, 65); // delta_key=5 → 旧序列被替换为 [65, 67]
    assert_eq!(sequence_keys(&editor), vec![65, 67]);
}

#[test]
fn test_dragging_selection_horizontal_move_keeps_sequence() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    setup_dragging_selection(&mut editor, &[0]);

    drag_to(&mut editor, 0.0, 63);
    assert_eq!(sequence_keys(&editor), vec![63]);

    // 纯水平移动（key 不变）：不重建序列（序列内容保持不变）
    drag_to(&mut editor, 500.0, 63);
    assert_eq!(sequence_keys(&editor), vec![63]);
}

#[test]
fn test_dragging_selection_back_to_original_key_clears_sequence() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    setup_dragging_selection(&mut editor, &[0]);

    drag_to(&mut editor, 0.0, 63);
    assert_eq!(sequence_keys(&editor), vec![63]);

    // 拖回原位（delta_key=0）：清空序列，不再发声
    drag_to(&mut editor, 0.0, 60);
    assert!(
        editor.editor_state.interaction.preview_sequence.is_empty(),
        "回到原位应清空预览序列"
    );
}

#[test]
fn test_dragging_selection_ghost_key_clamped_to_visible_range() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    setup_dragging_selection(&mut editor, &[0]);

    // 鼠标下移到 key=200：delta_key=140，ghost key=200 超出可见范围（max_key=127）→ clamp
    drag_to(&mut editor, 0.0, 200);
    assert_eq!(sequence_keys(&editor), vec![127]);
}

#[test]
fn test_dragging_selection_copy_also_triggers_sequence() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    let note_count = editor.editor_state.data.current_track_note_count();
    let drag = DragState::from_indices([0], note_count, 0, 60);
    editor.editor_state.interaction.edit_state =
        EditState::DraggingSelectionCopy { drag_state: drag };

    drag_to(&mut editor, 0.0, 63);
    assert_eq!(sequence_keys(&editor), vec![63], "复制拖动同样应有发声反馈");
}

#[test]
fn test_dragging_selection_release_clears_sequence() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    setup_dragging_selection(&mut editor, &[0]);

    drag_to(&mut editor, 0.0, 63);
    assert!(!editor.editor_state.interaction.preview_sequence.is_empty());

    // 松手：剩余未弹出的试听音符作废
    editor.handle_released();
    assert!(
        editor.editor_state.interaction.preview_sequence.is_empty(),
        "松手应清空预览序列"
    );
}

#[test]
fn test_take_audio_actions_drains_sequence_by_timing() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(480.0, 62, 100.0)],
    );
    setup_dragging_selection(&mut editor, &[0, 1]);

    drag_to(&mut editor, 0.0, 63); // 序列：tick 0 → 63（立即）、tick 480 → 65（+500ms）

    // 第一帧：首个音符（play_at 已到）立即弹出
    let actions = editor.take_audio_actions();
    assert_eq!(actions.len(), 1, "第一帧应弹出一个音符");
    assert!(
        matches!(
            actions.first(),
            Some(AudioAction::PlayNote { key: 63, velocity: 100 })
        ),
        "第一帧应弹出序列第一个音符（tick 0 的 ghost key）"
    );

    // 第二帧：第二个音符 500ms 后才到，未到期不应弹出
    assert!(
        editor.take_audio_actions().is_empty(),
        "未到 play_at 的音符不应弹出"
    );

    // 模拟时间流逝：把剩余音符的 play_at 拨到过去，下一帧应弹出
    for note in &mut editor.editor_state.interaction.preview_sequence {
        note.play_at = Instant::now() - Duration::from_secs(1);
    }
    let actions = editor.take_audio_actions();
    assert_eq!(actions.len(), 1);
    assert!(
        matches!(
            actions.first(),
            Some(AudioAction::PlayNote { key: 65, velocity: 100 })
        ),
        "play_at 到期后应弹出序列第二个音符"
    );
}
