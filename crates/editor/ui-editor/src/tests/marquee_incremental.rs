//! 框选**增量更新**正确性测试（`update_selection` 的 delta 路径）。
//!
//! `update_selection` 首帧全量重建，之后用 `rect_subtract` 切薄条只查 delta 区域，
//! 避免每帧 O(N) 全量扫描。增量路径有两类固有缺陷，本文件把二者钉死：
//!
//! 1. **key 轴 strip 边界 off-by-one**：key 为闭区间时上 strip 应切到
//!    `ic_k_min - 1`。写成 `ic_k_min` 会让 strip 与 inner 重叠一行，该行已选中音符
//!    被剔除，而新增路径不会补回（交集不属于 `new − old`）→ 框缩小时边界行静默丢失。
//! 2. **跨边界长音符被误剔**：差集按「矩形相减」切条，而命中语义是「与选框重叠」。
//!    跨越 remove 薄条、同时与新选框重叠的长音符会落入差集，需按新选框二次过滤。
//!
//! 另有终局不变式：**增量更新的结果必须恒等于对最终选框做一次全量重建**。

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;

/// 设置选框并触发**全量重建**（清空增量缓存）
fn select_full(
    editor: &mut Editor,
    start_tick: f32,
    current_tick: f32,
    start_key: u16,
    current_key: u16,
) {
    editor.cached_selection_bounds.set(None);
    editor.selected_bounds.set(None);
    editor.editor_state.interaction.selected_notes.clear();
    set_rect(editor, start_tick, current_tick, start_key, current_key);
}

/// 设置选框并触发**增量更新**（保留上一帧边界缓存）
fn select_delta(
    editor: &mut Editor,
    start_tick: f32,
    current_tick: f32,
    start_key: u16,
    current_key: u16,
) {
    set_rect(editor, start_tick, current_tick, start_key, current_key);
}

fn set_rect(
    editor: &mut Editor,
    start_tick: f32,
    current_tick: f32,
    start_key: u16,
    current_key: u16,
) {
    editor.editor_state.interaction.edit_state = EditState::Selecting {
        start_tick,
        start_key,
        current_tick,
        current_key,
        start_y: 0.0,
        current_y: 0.0,
    };
    editor.update_selection();
}

fn selected_sorted(editor: &Editor) -> Vec<usize> {
    let mut v: Vec<usize> = editor
        .editor_state
        .interaction
        .selected_notes
        .iter()
        .collect();
    v.sort_unstable();
    v
}

/// 缩小选框跨越长音符：长音符与新选框**仍有重叠**，必须保持选中；
/// 完全落入被移除区域的短音符必须取消选中。
#[test]
fn test_shrink_across_long_note_keeps_it_selected() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 2000.0), // 索引 0：长音符 [0, 2000)
            Note::new(0.0, 61, 100.0),  // 索引 1：短音符 [0, 100)，仅落在被移除区域
        ],
    );

    // 步骤 A：框 [0, 2000) —— 两条都选中
    select_full(&mut editor, 0.0, 2000.0, 60, 61);
    assert_eq!(
        selected_sorted(&editor),
        vec![0, 1],
        "初始框应同时选中长音符与短音符"
    );

    // 步骤 B：缩到 [1000, 2500) —— 走增量 delta 路径
    select_delta(&mut editor, 1000.0, 2500.0, 60, 61);
    let picked = selected_sorted(&editor);
    assert!(
        picked.contains(&0),
        "长音符 [0,2000) 与**新选框** [1000,2500) 仍重叠，必须保持选中（误剔回归点）"
    );
    assert!(
        !picked.contains(&1),
        "短音符 [0,100) 完全落在被移除区域，必须取消选中（保护不得误伤正常剔除）"
    );
    assert_eq!(picked, vec![0]);
}

/// 缩小选框仅剩单行 key：该行（新旧交集）必须保持选中，仅上方那一行被剔除。
///
/// 这是 `rect_subtract` key 轴 ±1 的直接回归点。
#[test]
fn test_shrink_to_single_key_row_keeps_that_row() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 100.0), // 索引 0
            Note::new(0.0, 61, 100.0), // 索引 1（单行交集行）
            Note::new(0.0, 62, 100.0), // 索引 2
        ],
    );

    // 步骤 A：key 60..=62 全选
    select_full(&mut editor, 0.0, 100.0, 60, 62);
    assert_eq!(selected_sorted(&editor), vec![0, 1, 2]);

    // 步骤 B：缩到 key 61..=62 —— 交集仅有 key 61 一行
    select_delta(&mut editor, 0.0, 100.0, 61, 62);
    let picked = selected_sorted(&editor);
    assert_eq!(
        picked,
        vec![1, 2],
        "缩到 key 61..=62 时应只剔除 key 60：交集行(key 61)被 off-by-one 误剔即为回归"
    );

    // 步骤 C：缩到单行 key 61
    select_delta(&mut editor, 0.0, 100.0, 61, 61);
    assert_eq!(
        selected_sorted(&editor),
        vec![1],
        "缩到单行 key 61 时应只保留该行"
    );
}

/// 终局不变式：任意选框序列的增量结果 == 对最终选框做一次全量重建的结果。
///
/// 这是比单点断言更强的守卫——任何 strip 边界 / 二次过滤的口径漂移都会在这里暴露。
#[test]
fn test_incremental_delta_equals_full_rebuild_for_any_sequence() {
    let notes = vec![
        Note::new(0.0, 60, 100.0),    // 0 短
        Note::new(0.0, 60, 2000.0),   // 1 长（跨多框）
        Note::new(500.0, 61, 600.0),  // 2
        Note::new(1800.0, 62, 100.0), // 3
        Note::new(0.0, 62, 100.0),    // 4
        Note::new(2500.0, 60, 50.0),  // 5
    ];
    // 选框序列：扩张 / 收缩 / 上下平移 / 单行 / 全量 交替
    let sequence: [(f32, f32, u16, u16); 7] = [
        (0.0, 3000.0, 60, 62),
        (1000.0, 3000.0, 61, 62),
        (1500.0, 2500.0, 60, 62),
        (0.0, 2000.0, 60, 61),
        (100.0, 1500.0, 60, 62),
        (2500.0, 2600.0, 60, 62),
        (0.0, 3000.0, 0, 127),
    ];

    let mut incremental = Editor::new();
    test_helpers::seed_notes(&mut incremental, 1, 0, &notes);
    // 首帧全量，其后全部走 delta
    select_full(
        &mut incremental,
        sequence[0].0,
        sequence[0].1,
        sequence[0].2,
        sequence[0].3,
    );
    assert_eq!(
        selected_sorted(&incremental),
        full_rebuild_result(&notes, sequence[0]),
        "首帧应与全量重建一致"
    );

    for (i, &rect) in sequence.iter().enumerate().skip(1) {
        select_delta(&mut incremental, rect.0, rect.1, rect.2, rect.3);
        assert_eq!(
            selected_sorted(&incremental),
            full_rebuild_result(&notes, rect),
            "第 {i} 步增量结果与全量重建不一致（选框 {rect:?}）"
        );
    }

    // 反向守卫：序列必须真的产生过非空选区，否则"两边都空"会假绿
    assert!(
        !selected_sorted(&incremental).is_empty(),
        "对照序列必须至少命中音符，否则断言无意义"
    );
}

/// 用一个全新 editor 对同一选框做一次全量重建，取期望结果
fn full_rebuild_result(notes: &[Note], rect: (f32, f32, u16, u16)) -> Vec<usize> {
    let mut fresh = Editor::new();
    test_helpers::seed_notes(&mut fresh, 1, 0, notes);
    select_full(&mut fresh, rect.0, rect.1, rect.2, rect.3);
    selected_sorted(&fresh)
}
