//! 框选（marquee）命中口径测试：tick 轴半开、key 轴闭。
//!
//! **分层原则**：本文件测「命中口径层」——直接写入 `EditState::Selecting` 的
//! 边界值后驱动 `update_selection`，不经过按下/移动的量化写入路径（那属于
//! `marquee_precision.rs` 的职责）。这样命中语义的断言与量化实现解耦，
//! 量化方案调整不会造成本文件"假失败"。
//!
//! 核心口径（见 `drag::selection::marquee_hits`）：
//! - **重叠即选中**：音符与选框内部有交叠即命中；
//! - **仅边界相触不算选中**：起点恰等于框右边界、终点恰等于框左边界的音符排除；
//! - **key 轴闭区间**：key 是离散格，整格覆盖。
//!
//! **两条实现路径必须同口径**：有空间索引（> 20 万音符工程）走
//! `NoteSpatialIndex::update_query_marquee`，无索引走 `ChunkedList` 窗口扫描 +
//! `marquee_hits`。任一路径口径漂移都会造成「工程规模不同 → 框选结果不同」。

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;

/// 首尾相接的三条边界音符（key 60）：
/// - `0`: C = `[0, 100)`
/// - `1`: A = `[100, 200)`
/// - `2`: B = `[200, 300)`
fn boundary_notes() -> Vec<Note> {
    vec![
        Note::new(0.0, 60, 100.0),
        Note::new(100.0, 60, 100.0),
        Note::new(200.0, 60, 100.0),
    ]
}

/// 以显式边界驱动一次框选命中计算（绕过量化写入路径）
///
/// 每次调用都重置增量缓存 → 强制走**全量重建**路径，把 `rect_subtract`
/// 增量差集的行为隔离出去（由 `marquee_incremental.rs` 单独覆盖）。
fn select_rect(
    editor: &mut Editor,
    start_tick: f32,
    current_tick: f32,
    start_key: u16,
    current_key: u16,
) {
    editor.cached_selection_bounds.set(None);
    editor.selected_bounds.set(None);
    editor.editor_state.interaction.selected_notes.clear();
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

/// 强制安装空间索引。
///
/// 小数据量下 `ensure_spatial_index` 默认不建树（阈值 20 万），走窗口兜底；
/// 本辅助用于显式覆盖「索引路径」，与无索引路径做同口径对照。
fn force_spatial_index(editor: &Editor) {
    let notes = editor.editor_state.data.current_track_notes();
    let refs: Vec<lumino_note_core::NoteRef> = notes
        .iter()
        .enumerate()
        .map(|(i, n)| lumino_note_core::NoteRef {
            tick: n.start_tick as f32,
            key: n.key as u16,
            length: (n.end_tick - n.start_tick) as f32,
            index: i,
        })
        .collect();
    *editor.spatial.note_index.borrow_mut() =
        Some(lumino_note_core::NoteSpatialIndex::from_note_refs(&refs));
    editor.spatial.note_index_dirty.set(false);
}

/// 跑一组口径断言（对同一 editor 的两种实现路径复用）
fn assert_half_open_semantics(editor: &mut Editor) {
    // 1) 框 [100, 200)：C 终点=100 与 B 起点=200 仅边界相触 → 排除；A 内部交叠 → 入选
    select_rect(editor, 100.0, 200.0, 60, 60);
    assert_eq!(
        editor.selected_notes_count(),
        1,
        "框 [100,200) 应只选中 A[100,200)：终点恰等左边界(C)与起点恰等右边界(B)不得入选"
    );
    assert!(
        editor.editor_state.interaction.selected_notes.contains(&1),
        "入选的必须是 A[100,200)（索引 1）"
    );

    // 2) 框 [0, 100)：C 入选；A 起点恰等于右边界 100 → 排除（闭区间下会误选）
    select_rect(editor, 0.0, 100.0, 60, 60);
    assert_eq!(
        editor.selected_notes_count(),
        1,
        "框 [0,100) 应只选中 C：起点恰等于框右边界的 A 不得入选（框缘误选回归点）"
    );
    assert!(editor.editor_state.interaction.selected_notes.contains(&0));

    // 3) 零宽框（拖动后回到按下 tick，未形成区间）
    //    - tick 落在 A[100,200) **内部** → 覆盖命中，仍须选中（半开 ≠ 排除内部）
    select_rect(editor, 150.0, 150.0, 60, 60);
    assert_eq!(
        editor.selected_notes_count(),
        1,
        "零宽框 [150,150)：tick 150 落在 A[100,200) 内部，覆盖命中必须保留"
    );
    //    - tick 恰在音符边界 100 上 → 两侧音符均仅边界相触 → 全排除
    select_rect(editor, 100.0, 100.0, 60, 60);
    assert_eq!(
        editor.selected_notes_count(),
        0,
        "零宽框 [100,100)：C 终点=100 与 A 起点=100 均仅边界相触，必须全排除"
    );

    // 4) 跨边界长音符：内部交叠 → 必须入选（半开 ≠ 包含）
    let mut wide = Editor::new();
    test_helpers::seed_notes(&mut wide, 1, 0, &[Note::new(0.0, 60, 1000.0)]);
    if editor.spatial.note_index.borrow().is_some() {
        force_spatial_index(&wide);
    }
    select_rect(&mut wide, 500.0, 600.0, 60, 60);
    assert_eq!(
        wide.selected_notes_count(),
        1,
        "长音符 [0,1000) 跨越框 [500,600) 且内部交叠，必须入选"
    );

    // 5) key 轴闭区间：key 上下边界都必须命中
    let mut keys = Editor::new();
    test_helpers::seed_notes(
        &mut keys,
        1,
        0,
        &[
            Note::new(0.0, 60, 100.0),
            Note::new(0.0, 61, 100.0),
            Note::new(0.0, 62, 100.0),
        ],
    );
    if editor.spatial.note_index.borrow().is_some() {
        force_spatial_index(&keys);
    }
    select_rect(&mut keys, 0.0, 100.0, 60, 62);
    assert_eq!(
        keys.selected_notes_count(),
        3,
        "key 轴闭区间：60/61/62 三行都须命中"
    );
    select_rect(&mut keys, 0.0, 100.0, 61, 61);
    assert_eq!(keys.selected_notes_count(), 1, "key 轴单值只命中该行");
}

/// 无空间索引路径（`ChunkedList` 窗口扫描兜底）
#[test]
fn test_marquee_half_open_semantics_via_window_scan() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &boundary_notes());
    assert!(
        editor.spatial.note_index.borrow().is_none(),
        "小数据量默认不建空间索引，本用例覆盖窗口扫描兜底路径"
    );
    assert_half_open_semantics(&mut editor);
}

/// 有空间索引路径（> 20 万音符工程的快速路径）
#[test]
fn test_marquee_half_open_semantics_via_spatial_index() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &boundary_notes());
    force_spatial_index(&editor);
    assert_half_open_semantics(&mut editor);
}

/// 两条路径结果必须逐索引一致（防「工程规模不同 → 框选结果不同」的口径分裂）
#[test]
fn test_marquee_both_paths_agree_on_boundary_cases() {
    // 覆盖框边界与音符边界精确对齐 / 半格交错 / 跨边界多种组合
    let notes = vec![
        Note::new(0.0, 60, 100.0),
        Note::new(100.0, 60, 100.0),
        Note::new(200.0, 60, 100.0),
        Note::new(50.0, 61, 400.0), // 跨多框的长音符
        Note::new(300.0, 62, 10.0),
    ];
    let rects = [
        (0.0_f32, 100.0_f32, 60_u16, 62_u16),
        (100.0, 200.0, 60, 60),
        (50.0, 350.0, 60, 62),
        (100.0, 100.0, 60, 62),
        (0.0, 1000.0, 0, 127),
        (250.0, 260.0, 61, 61),
    ];

    let collect = |force_index: bool| -> Vec<Vec<usize>> {
        rects
            .iter()
            .map(|&(t0, t1, k0, k1)| {
                let mut editor = Editor::new();
                test_helpers::seed_notes(&mut editor, 1, 0, &notes);
                if force_index {
                    force_spatial_index(&editor);
                }
                select_rect(&mut editor, t0, t1, k0, k1);
                let mut v: Vec<usize> = editor
                    .editor_state
                    .interaction
                    .selected_notes
                    .iter()
                    .collect();
                v.sort_unstable();
                v
            })
            .collect()
    };

    let via_window = collect(false);
    let via_index = collect(true);
    assert_eq!(
        via_window, via_index,
        "窗口扫描与空间索引两条路径的框选结果必须完全一致"
    );
    // 反向守卫：本用例必须真的选中过音符（否则"两路径都空"会假绿）
    assert!(
        via_window.iter().any(|v| !v.is_empty()),
        "对照用例必须至少有一组命中，否则断言无意义"
    );
}
