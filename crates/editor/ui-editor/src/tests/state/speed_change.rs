//! 音符变速功能测试（`apply_speed_change`）
//!
//! 从 `state.rs` 拆分而来。

use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;

/// 测试空音符列表变速
#[test]
fn test_speed_change_empty_notes() {
    let mut editor = Editor::new();
    let modified = editor.apply_speed_change(0.5);
    assert_eq!(modified, 0);
}

/// 测试全部音符变速
#[test]
fn test_speed_change_all_notes() {
    let mut editor = Editor::new();
    // 音符A: tick=0, length=480
    // 音符B: tick=600, length=240
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(600.0, 62, 240.0)],
    );

    let modified = editor.apply_speed_change(0.5);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    // 以最早 tick(0) 为锚点缩放
    // A: tick'=0+(0-0)*0.5=0, length'=240
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").length - 240.0).abs() < f32::EPSILON
    );
    // B: tick'=0+(600-0)*0.5=300, length'=120
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 300.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").length - 120.0).abs() < f32::EPSILON
    );
}

/// 测试仅选中的音符变速
#[test]
fn test_speed_change_selected_notes_only() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 480.0),
            Note::new(600.0, 62, 240.0),
            Note::new(1200.0, 64, 120.0),
        ],
    );

    // 只选中第 1 和第 3 个音符
    editor.editor_state.interaction.selected_notes.insert(0);
    editor.editor_state.interaction.selected_notes.insert(2);

    let modified = editor.apply_speed_change(2.0);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    // A 选中: tick'=0, length'=960
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").length - 960.0).abs() < f32::EPSILON
    );
    // B 未选中: 不变
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 600.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").length - 240.0).abs() < f32::EPSILON
    );
    // C 选中: tick'=0+(1200-0)*2=2400, length'=240
    assert!(
        (data.get_note_view(2).expect("第 3 个音符视图应存在").tick - 2400.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(2).expect("第 3 个音符视图应存在").length - 240.0).abs() < f32::EPSILON
    );
}

/// 测试变速时最小长度限制
#[test]
fn test_speed_change_clamp_to_min_length() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 10.0)]);

    let modified = editor.apply_speed_change(0.01);
    assert_eq!(modified, 1);

    let data = &editor.editor_state.data;
    // tick 缩放: 100+(100-100)*0.01=100
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 100.0).abs() < f32::EPSILON
    );
    // 最小长度为 1 tick
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").length - 1.0).abs() < f32::EPSILON
    );
}

/// 测试变速因子为 1 时无变化
#[test]
fn test_speed_change_no_op_when_factor_is_one() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    let modified = editor.apply_speed_change(1.0);
    assert_eq!(modified, 0);

    let data = &editor.editor_state.data;
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").length - 480.0).abs() < f32::EPSILON
    );
}

/// 测试变速后撤销/重做
#[test]
fn test_speed_change_undo_redo() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(600.0, 62, 240.0)],
    );

    let modified = editor.apply_speed_change(0.5);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").length - 240.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 300.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").length - 120.0).abs() < f32::EPSILON
    );

    // 撤销
    let undo_result = editor.undo();
    assert!(undo_result);

    let data = &editor.editor_state.data;
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").length - 480.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 600.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").length - 240.0).abs() < f32::EPSILON
    );
}

/// 关键测试：尾部贴合的音符变速后仍然贴合
#[test]
fn test_speed_change_preserves_adjacent_notes() {
    let mut editor = Editor::new();
    // A: tick=100, length=200 → 结束于 300
    // B: tick=300, length=150 → 开始于 300
    // A 和 B 尾部贴合
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(100.0, 60, 200.0), Note::new(300.0, 62, 150.0)],
    );

    let modified = editor.apply_speed_change(0.5);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    // A: tick'=100+(100-100)*0.5=100, length'=100 → 结束于 200
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 100.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").length - 100.0).abs() < f32::EPSILON
    );
    // B: tick'=100+(300-100)*0.5=200, length'=75 → 开始于 200
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 200.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").length - 75.0).abs() < f32::EPSILON
    );

    // 验证贴合: A.end == B.start
    let a_end = data.get_note_view(0).expect("第 1 个音符视图应存在").tick
        + data.get_note_view(0).expect("第 1 个音符视图应存在").length;
    let b_start = data.get_note_view(1).expect("第 2 个音符视图应存在").tick;
    assert!(
        (a_end - b_start).abs() < f32::EPSILON,
        "尾部贴合关系被破坏: A.end={}, B.start={}",
        a_end,
        b_start
    );
}

/// 验证有间隙的音符保持相对间隙比例
#[test]
fn test_speed_change_preserves_gap_ratio() {
    let mut editor = Editor::new();
    // A: tick=0, length=100 → 结束于 100
    // B: tick=200, length=100 → 开始于 200
    // 间隙 = 100 ticks
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(200.0, 62, 100.0)],
    );

    let modified = editor.apply_speed_change(2.0);
    assert_eq!(modified, 2);

    let data = &editor.editor_state.data;
    // A: tick'=0, length'=200 → 结束于 200
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").tick - 0.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(0).expect("第 1 个音符视图应存在").length - 200.0).abs() < f32::EPSILON
    );
    // B: tick'=0+(200-0)*2=400, length'=200
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").tick - 400.0).abs() < f32::EPSILON
    );
    assert!(
        (data.get_note_view(1).expect("第 2 个音符视图应存在").length - 200.0).abs() < f32::EPSILON
    );

    // 验证间隙比例: 原始间隙=100, 缩放后间隙=200
    let original_gap = 200.0 - (0.0 + 100.0); // B.start - A.end
    let new_gap = data.get_note_view(1).expect("第 2 个音符视图应存在").tick
        - (data.get_note_view(0).expect("第 1 个音符视图应存在").tick
            + data.get_note_view(0).expect("第 1 个音符视图应存在").length);
    assert!(
        (new_gap - original_gap * 2.0).abs() < f32::EPSILON,
        "间隙比例被破坏: 原始={}, 新={}",
        original_gap,
        new_gap
    );
}
