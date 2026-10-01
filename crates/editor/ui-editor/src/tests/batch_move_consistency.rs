//! 批量框选移动一致性回归：显示（幽灵/框选）与内存（文档）必须一致
//!
//! 背景 BUG：批量框选后移动音符、多次重复后出现显示与内存分叉，框选错位。
//! 根因三处（见各测试注释）：
//! 1. 异步提交按值解析同值多份碰撞到同一索引 → 一个双移、一个不动；
//! 2. 空白点击提交（飞行中）后立即新框选按旧文档查询 → 框选丢失/错位；
//! 3. 飞行中再次拖动直接累积污染飞行中的 pending → 视觉累积与内存快照分叉。
//!
//! 本文件把三处钉死，并覆盖“多次重复循环”终局不变式。

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

/// 模拟批量拖动入口（与 `pending_drag.rs` 同语义）：首次 push 历史 + 进入拖动
fn start_batch_drag(
    editor: &mut Editor,
    indices: impl IntoIterator<Item = usize>,
    delta_tick: i64,
    delta_key: i16,
) {
    // 生产链路不推快照，仅提交推 MoveOp
    let note_count = editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices(indices, note_count, 0, 60);
    drag.set_delta(delta_tick, delta_key);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
}

fn track_ticks(editor: &Editor, track: usize) -> Vec<u32> {
    editor
        .editor_state
        .data
        .track_notes(track)
        .iter()
        .map(|n| n.start_tick)
        .collect()
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

/// 全量框选（清空增量缓存后重建），返回选中索引
fn box_select_full(editor: &mut Editor, t0: f32, t1: f32, k0: u16, k1: u16) -> Vec<usize> {
    editor.cached_selection_bounds.set(None);
    editor.selected_bounds.set(None);
    editor.editor_state.interaction.selected_notes.clear();
    editor.editor_state.interaction.edit_state = EditState::Selecting {
        start_tick: t0,
        start_key: k0,
        current_tick: t1,
        current_key: k1,
        start_y: 0.0,
        current_y: 0.0,
    };
    editor.update_selection();
    selected_sorted(editor)
}

// ── 根因 1：同值多份必须各移动一次 ─────────────────────────────

#[test]
fn test_batch_move_duplicate_values_each_moves_once() {
    // 两个完全相同的音符（同 tick/key/len/vel/chan）：旧实现按值解析都命中索引 0，
    // 导致索引 0 被双移（0→200）、索引 1 不动，内存变为 [0,200] 而视觉为 [100,100]。
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(0.0, 60, 240.0, 100, 0),
        ],
    );
    editor.selection_insert(0);
    editor.selection_insert(1);

    start_batch_drag(&mut editor, [0, 1], 100, 0);
    editor.handle_released();
    assert!(editor.commit_pending_drag(), "应启动异步提交");
    editor.drain_async_commit();

    assert_eq!(
        track_ticks(&editor, 0),
        vec![100, 100],
        "同值双音符应各移动一次，而非一个双移、一个不动"
    );
}

#[test]
fn test_batch_move_duplicate_reselect_keeps_both() {
    // 同值双音符移动后选中应跟随到两个新值（两份都选中），旧实现按新值定位都命中同一索引丢一份。
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(0.0, 60, 240.0, 100, 0),
        ],
    );
    editor.selection_insert(0);
    editor.selection_insert(1);

    start_batch_drag(&mut editor, [0, 1], 100, 0);
    editor.handle_released();
    assert!(editor.commit_pending_drag());
    editor.drain_async_commit();
    // drain 后不清空选区时应保留两份选中（poll 按新值重选，份数语义）
    // 此处模拟保留旧框场景：提交前选中即 pending 集，drain 后应重选到新值
    // 由于 commit 后本测试未清空，检查选中数即可（需先重建选中以观察）：
    // 直接断言文档正确即覆盖核心，选中份数由下式补充验证
    assert_eq!(track_ticks(&editor, 0), vec![100, 100]);
    // 按新值重选两份：手动构造 olds 验证 reselect 路径不丢份
    // （通过再次全量框选命中两份间接验证索引可达）
    let sel = box_select_full(&mut editor, 0.0, 300.0, 60, 60);
    assert_eq!(sel, vec![0, 1], "移动后框选应仍命中两份同值音符");
}

// ── 根因 2：飞行中（提交未落盘）新框选必须按视觉（幽灵）位置命中 ──

#[test]
fn test_flight_new_marquee_aligns_with_ghost() {
    // 空白点击提交后飞行中立即新框选：显示在幽灵位置 [1000,1500,2800]，
    // 旧实现按旧文档 [0,500,1800] 查询，框 [900,3000) 只命中 1 个，落盘后重映射按旧值查找直接清空。
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 100.0),
            Note::new(500.0, 61, 100.0),
            Note::new(1800.0, 62, 100.0),
        ],
    );
    assert_eq!(box_select_full(&mut editor, 0.0, 3000.0, 60, 62).len(), 3);

    let sel = editor.get_selected_indices();
    start_batch_drag(&mut editor, sel, 1000, 0);
    editor.handle_released();
    assert!(editor.commit_pending_drag(), "应启动飞行");
    assert!(
        editor.editor_state.data.has_pending_commit(),
        "应处于飞行中"
    );
    // 模拟空白点击 flush 后半：清空选区（真实 flush 行为），文档仍是旧值
    editor.selection_clear();
    assert_eq!(
        track_ticks(&editor, 0),
        vec![0, 500, 1800],
        "飞行中文档仍是旧值（幽灵显示新值）"
    );

    // 用户框住视觉上的 3 个（幽灵 [1000,1500,2800] 全在 [900,3000) 内）
    let flight_sel = box_select_full(&mut editor, 900.0, 3000.0, 60, 62);
    assert_eq!(
        flight_sel,
        vec![0, 1, 2],
        "飞行中框选必须按幽灵位置命中 3 个，而非按旧文档只命中 1 个"
    );

    editor.drain_async_commit();
    assert_eq!(
        track_ticks(&editor, 0),
        vec![1000, 1500, 2800],
        "落盘后内存应为旧值+1000"
    );
    assert_eq!(
        selected_sorted(&editor),
        vec![0, 1, 2],
        "落盘后选区必须保留视觉框住的 3 个（幽灵值重定位），不得清空/错位"
    );
}

// ── 根因 3：飞行中再次拖动必须串行化，视觉总量与内存总量一致 ────

#[test]
fn test_flight_second_drag_serializes_consistently() {
    // 飞行中（100 未落盘）再次拖动 50：旧实现直接累积 pending 到 150，
    // 但飞行 ops 已按 100 快照，落盘只应用 100，50 被吞（视觉 150 vs 内存 100）。
    // 修复后串行化为两阶段：先落 100，再存 50 新基准，最终总量 150，两次落盘。
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(480.0, 62, 240.0, 100, 0),
        ],
    );
    editor.selection_insert(0);
    editor.selection_insert(1);

    start_batch_drag(&mut editor, [0, 1], 100, 0);
    editor.handle_released();
    assert!(editor.commit_pending_drag());
    assert!(editor.editor_state.data.has_pending_commit());

    // 飞行中第二次拖动（直接 API 模拟，真实链路经 pressed 串行化同样收敛到两阶段）
    start_batch_drag(&mut editor, [0, 1], 50, 0);
    editor.handle_released();
    // 串行化后：第一次的 100 已在松手时 drain 落盘，pending 应为新基准的 50
    assert_eq!(
        track_ticks(&editor, 1),
        vec![100, 580],
        "串行化后第一次位移应已落盘"
    );
    assert_eq!(
        editor
            .pending_drag_state
            .as_ref()
            .expect("应有新基准 pending")
            .delta_tick,
        50,
        "新基准 pending 应仅含第二次位移"
    );

    assert!(editor.commit_pending_drag(), "新基准 pending 应可提交");
    editor.drain_async_commit();
    assert_eq!(
        track_ticks(&editor, 1),
        vec![150, 630],
        "两次落盘总量应等于视觉总量（100+50）"
    );
    assert!(
        editor.pending_drag_state.is_none(),
        "全部落盘后 pending 应清空"
    );
    // 显示一致性：无 pending 时幽灵增量为空
    assert!(
        editor.build_ghost_delta_positions(&[0, 1]).is_empty(),
        "落盘后不应再有幽灵增量（显示与内存一致）"
    );
}

// ── 终局不变式：多次重复框选→移动→提交循环，显示恒等于内存 ──────

#[test]
fn test_repeated_box_move_cycles_display_matches_memory() {
    // 用户原始报告路径：批量框选后移动，多次重复。每次都 drain 落盘，
    // 每次验证内存步进正确、无幽灵残留、重框选无错位。
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 100.0),
            Note::new(500.0, 61, 100.0),
            Note::new(1800.0, 62, 100.0),
        ],
    );
    for round in 0..3 {
        let sel = box_select_full(&mut editor, 0.0, 3000.0 + 100.0 * round as f32, 60, 62);
        assert_eq!(sel.len(), 3, "第 {round} 轮应选中 3 个");

        start_batch_drag(&mut editor, sel, 100, 0);
        editor.handle_released();
        assert!(editor.commit_pending_drag());
        editor.drain_async_commit();
        editor.selection_clear();

        let expect = 100 * (round + 1) as u32;
        assert_eq!(
            track_ticks(&editor, 0),
            vec![expect, 500 + expect, 1800 + expect],
            "第 {round} 轮内存应步进 +100"
        );
        assert!(
            editor.pending_drag_state.is_none(),
            "第 {round} 轮落盘后 pending 应清空"
        );
        assert!(
            editor.build_ghost_delta_positions(&[0, 1, 2]).is_empty(),
            "第 {round} 轮落盘后幽灵应清空（显示与内存一致）"
        );
        // 落盘后按新位置重框选应仍命中 3 个（框选无错位）
        let resel = box_select_full(&mut editor, 0.0, 3000.0 + expect as f32, 60, 62);
        assert_eq!(resel.len(), 3, "第 {round} 轮落盘后重框选应仍命中 3 个");
        editor.selection_clear();
    }
}

// ── 选择框视觉跟随：pending 期间框选框必须叠加 delta ─────────────

#[test]
fn test_selection_box_follows_ghost_during_pending() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(500.0, 61, 100.0)],
    );
    assert_eq!(box_select_full(&mut editor, 0.0, 1000.0, 60, 61).len(), 2);
    let raw_rect = editor.get_selection_box_bounds().expect("应有原始框");

    start_batch_drag(&mut editor, [0, 1], 200, 0);
    editor.handle_released();
    assert!(editor.pending_drag_state.is_some(), "松手后应有 pending");
    // 文档未动（幽灵方案），但选择框必须跟随到幽灵位置
    assert_eq!(
        track_ticks(&editor, 0),
        vec![0, 500],
        "pending 期间文档不得变动"
    );
    let ghost_rect = editor.get_selection_box_bounds().expect("应有幽灵框");
    assert!(
        (ghost_rect.0 - raw_rect.0 - 200.0 * editor.editor_state.view.zoom_x).abs() < 1.0,
        "幽灵框 X 应比原始框右移 delta 对应的屏幕距离（跟随鼠标），实际 {ghost_rect:?} vs {raw_rect:?}"
    );

    editor.drain_async_commit();
    // 注意：此处直接 commit 未经 flush 清空，drain 后选中按新值重选，框应落在新位置且与幽灵框一致
    let committed_rect = editor.get_selection_box_bounds().expect("落盘后应有框");
    assert!(
        (committed_rect.0 - ghost_rect.0).abs() < 1.0
            && (committed_rect.1 - ghost_rect.1).abs() < 1.0,
        "落盘后框（内存位置）应与 pending 期间幽灵框（显示位置）一致，否则显示与内存分叉"
    );
}
