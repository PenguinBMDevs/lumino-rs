//! 工程走带「历史链 + 选区生命周期」回归。
//!
//! 缺陷背景（走带撤销链整条从未通过）：
//! 1. `arrange_move_notes` / `arrange_apply_speed_change` 走 `insert_note` /
//!    `remove_note`，而这两个 API 的契约是「调用方需先 `push_history()`」——批量
//!    操作漏 push，导致走带移动 / 变速**根本不进历史栈**，Ctrl+Z 撤不掉。
//! 2. 更糟：两者在「实际未修改任何音符」时调用 `discard_last_history()` 兜底，
//!    但它们自己从未 push 过——这一 discard 丢掉的是**用户上一次真实编辑**的
//!    快照，表现为「Ctrl+Z 莫名少撤一步」。
//! 3. 撤销 / 重做只重映射卷帘选区，零 `arrange_selection` 引用；而走带选区在
//!    移动 / 变速后是**冻结精确集合**（位置快照），撤销后位置全错 → 选区悬空，
//!    后续拖动 / 删除 / 复制全部作用不到音符且无任何提示。
//!
//! 修复：批量操作补 `push_history()`（失败才 discard，语义自洽）；`undo` / `redo`
//! 成功后清空走带**冻结**选区（矩形模式是活语义，保留）。

use crate::Editor;
use crate::edit_view::EditView;
use crate::note::Note;
use crate::tests::test_helpers::seed_notes;

/// 音轨音符值列表 `(start_tick, key)`（按值断言，避免索引漂移干扰）
fn track_values(editor: &Editor, track: usize) -> Vec<(u32, u8)> {
    editor
        .editor_state
        .data
        .track_notes(track)
        .iter()
        .map(|n| (n.start_tick, n.key))
        .collect()
}

fn editor_with_selection() -> Editor {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::from_raw(100.0, 60, 100.0, 100, 0),
            Note::from_raw(300.0, 60, 100.0, 100, 0),
        ],
    );
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(50, 450, 0, 127, 0, 0);
    editor
}

/// 走带拖动移动必须进历史栈：Ctrl+Z 能把音符撤回原位
#[test]
fn test_arrangement_move_is_undoable() {
    let mut editor = editor_with_selection();
    assert_eq!(editor.arrange_move_notes(1000, 0), 2, "应移动 2 个音符");
    assert_eq!(
        track_values(&editor, 0),
        vec![(1100, 60), (1300, 60)],
        "移动后应位于新位置"
    );

    assert!(editor.undo(), "撤销应成功（移动必须已进历史栈）");
    assert_eq!(
        track_values(&editor, 0),
        vec![(100, 60), (300, 60)],
        "撤销后音符应回到原位"
    );
}

/// 空移动（delta=0）不得吞掉用户之前的撤销点
///
/// 场景需要**两次**真实移动：零偏移移动若误 discard，吞掉的是最近一条
/// （第二次移动），表现为「撤销一次就回到中间态、再撤就没得撤」。
#[test]
fn test_noop_move_does_not_discard_previous_history() {
    let mut editor = editor_with_selection();
    assert_eq!(editor.arrange_move_notes(1000, 0), 2, "第一次移动");
    assert_eq!(editor.arrange_move_notes(1000, 0), 2, "第二次移动");
    assert_eq!(editor.arrange_move_notes(0, 0), 0, "零偏移应不移动");

    assert!(editor.undo(), "第一次撤销应撤掉第二次移动");
    assert!(
        editor.undo(),
        "第二次撤销应撤掉第一次移动（未被零偏移移动吞掉）"
    );
    assert_eq!(
        track_values(&editor, 0),
        vec![(100, 60), (300, 60)],
        "两次撤销后应回到最初位置"
    );
}

/// 批量变速必须进历史栈：Ctrl+Z 能把长度撤回
#[test]
fn test_arrangement_speed_change_is_undoable() {
    let mut editor = editor_with_selection();
    let modified = editor.arrange_apply_speed_change(2.0);
    assert!(modified > 0, "应变速若干音符");
    let after = editor.editor_state.data.track_notes(0)[0].length();

    assert!(editor.undo(), "撤销应成功（变速必须已进历史栈）");
    let restored = editor.editor_state.data.track_notes(0)[0].length();
    assert!(restored < after, "撤销后长度应恢复（{restored} < {after}）");
}

/// 撤销后走带冻结选区必须清空——否则选区悬空，后续操作静默失效
#[test]
fn test_undo_clears_frozen_arrange_selection() {
    let mut editor = editor_with_selection();
    editor.arrange_move_notes(1000, 0);
    let selection = &editor.editor_state.data.arrange_selection;
    assert!(selection.frozen().is_some(), "移动后选区应为冻结精确集合");

    assert!(editor.undo());
    assert!(
        editor
            .editor_state
            .data
            .arrange_selection
            .frozen()
            .is_none(),
        "撤销后冻结集必须清空（位置快照已失效）"
    );
    assert!(
        editor.editor_state.data.arrange_selection.is_empty(),
        "冻结集清空后不应残留矩形导致命中旧位置音符"
    );
}

/// 矩形模式（活语义）选区在撤销后保留——矩形描述的是区域，不是位置快照
#[test]
fn test_undo_keeps_rect_arrange_selection() {
    let mut editor = editor_with_selection();
    assert!(
        editor
            .editor_state
            .data
            .arrange_selection
            .frozen()
            .is_none(),
        "纯框选应为矩形模式（无冻结集）"
    );
    assert_eq!(editor.arrange_move_notes(1000, 0), 2);
    // 移动后重置为矩形模式，模拟「用户重新框选」
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(50, 450, 0, 127, 0, 0);

    assert!(editor.undo());
    assert!(
        !editor.editor_state.data.arrange_selection.is_empty(),
        "矩形模式选区是活语义，撤销后应保留"
    );
    assert!(
        editor
            .editor_state
            .data
            .arrange_selection
            .contains(0, 100, 60),
        "撤销后矩形应重新命中回到原位的音符"
    );
}

/// 走带全选：覆盖全部音轨与全部 tick 区间
#[test]
fn test_arrangement_select_all_covers_every_track() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        3,
        0,
        &[
            Note::from_raw(100.0, 60, 100.0, 100, 0),
            Note::from_raw(1000.0, 60, 100.0, 100, 0),
        ],
    );
    assert!(
        editor.editor_state.data.arrange_selection.is_empty(),
        "前置条件：初始无选区"
    );

    assert!(editor.arrange_select_all_notes(), "全选应生效");
    let selection = &editor.editor_state.data.arrange_selection;
    assert!(!selection.is_empty(), "全选后选区非空");
    assert!(
        selection.contains(0, 100, 60) && selection.contains(0, 1000, 60),
        "全选应命中当前轨首尾音符"
    );
    assert!(
        selection.contains(2, 0, 60),
        "全选应覆盖末位音轨（不受当前轨限制）"
    );
}

// ══════════════════════════════════════════════════════════════════
// 视图无关的选区解析（`resolve_selection`）—— 任务 D 收口后的统一入口
//
// 缺陷背景（两个 bug 叠加，且都与「入口层零视图仲裁」同源）：
// 1. **优先序错误**：旧实现 `if has_selection() { return; }` 让卷帘选区无条件
//    优先，走带选区被忽略。而菜单启用条件是 `卷帘非空 || 走带非空`——两套选区
//    彼此独立、互不清理，从卷帘切到走带后卷帘选区仍留存，于是**菜单能点、
//    导出却是另一套选区**（用户框了 A，拿到的是 B）。
// 2. **坐标空间错误**：走带分支把文档音轨索引当视觉轨传进 `contains`。选区
//    （含冻结集）存的是视觉轨，`track_visual_order` 非恒等时判定全错——与
//    2025-07 修过的 `arrangement-y-axis-movement` 是同一个坑换个入口复现。
//
// 收口后：调用方必须显式传 `EditView`，无法绕过视图直接读某一套选区。
// ══════════════════════════════════════════════════════════════════

/// 收集结果按文档轨分组的 `(轨, 音符数)` 列表（走带视图优先）
fn export_shape(editor: &Editor) -> Vec<(usize, usize)> {
    editor
        .resolve_selection(EditView::Arrangement)
        .into_tracks()
        .into_iter()
        .map(|(t, notes)| (t, notes.len()))
        .collect()
}

/// 回归 1：走带模式下两套选区同时非空，必须取**走带**选区（视图优先）
#[test]
fn test_export_prefers_arrangement_selection_over_roll() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        3,
        0,
        &[
            Note::from_raw(0.0, 60, 10.0, 100, 0),    // 卷帘选区会选中它
            Note::from_raw(5000.0, 67, 10.0, 100, 0), // 走带选区会选中它
        ],
    );
    // 卷帘选区：索引 0
    editor.selection_insert(0);
    // 走带选区：只框 [5000, 5010)
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(5000, 5010, 0, 127, 0, 2);

    let notes = editor
        .resolve_selection(EditView::Arrangement)
        .into_tracks();
    assert_eq!(notes.len(), 1, "走带优先时应只返回走带选区命中的 1 条音轨");
    assert_eq!(
        (notes[0].0, notes[0].1[0].start_tick, notes[0].1[0].key),
        (0, 5000, 67),
        "必须取走带框选的音符（tick 5000），而非卷帘残留选区（tick 0）"
    );
}

/// 回归 2：走带选区为空但卷帘有选区时回退卷帘（与菜单 OR 启用条件一致）
#[test]
fn test_export_falls_back_to_roll_selection() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 1, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
    editor.selection_insert(0);
    assert!(
        editor.editor_state.data.arrange_selection.is_empty(),
        "前置条件：走带选区为空"
    );

    // 走带优先但走带空 → 回退卷帘（否则菜单可点却导不出，OR 语义被破坏）
    assert_eq!(
        export_shape(&editor),
        vec![(0, 1)],
        "走带选区为空时必须回退卷帘选区"
    );
    // 反向：卷帘优先 + 卷帘有选区 + 走带空 → 同样取卷帘
    assert_eq!(export_shape(&editor), vec![(0, 1)]);
}

/// 回归 3：非恒等 `track_visual_order` 下走带选区仍按**视觉轨**判定
///
/// 旧实现传 `track_idx as u16`（文档索引）→ 视觉序 [2,0,1] 时全错。
#[test]
fn test_export_uses_visual_track_not_document_index() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 3, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
    // `track_visual_order` 语义 = 视觉位置 → 文档轨索引
    // vec![2,0,1] → 视觉0=doc2、视觉1=doc0、视觉2=doc1：文档轨 0 位于**视觉轨 1**
    editor.editor_state.data.track_visual_order = vec![2, 0, 1];
    // 走带框选**视觉轨 1**（即文档轨 0 所在处）
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 10, 0, 127, 1, 1);

    let notes = editor
        .resolve_selection(EditView::Arrangement)
        .into_tracks();
    assert_eq!(
        notes.len(),
        1,
        "视觉轨判定应命中文档轨 0 的音符（旧实现传文档索引 0 会误判为视觉轨 0 → 漏检）"
    );
    assert_eq!(notes[0].0, 0, "命中的文档音轨索引应为 0");
}

/// 收口后的对称性：卷帘视图优先时取卷帘；两视图都有选区时按视图分派（不再是「卷帘无条件优先」）
#[test]
fn test_resolve_selection_is_view_symmetric() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        3,
        0,
        &[
            Note::from_raw(0.0, 60, 10.0, 100, 0),
            Note::from_raw(5000.0, 67, 10.0, 100, 0),
        ],
    );
    editor.selection_insert(0);
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(5000, 5010, 0, 127, 0, 2);

    // 卷帘优先 → 取卷帘选区（tick 0）
    let roll = editor.resolve_selection(EditView::PianoRoll).into_tracks();
    assert_eq!(roll.len(), 1);
    assert_eq!(
        (roll[0].0, roll[0].1[0].start_tick, roll[0].1[0].key),
        (0, 0, 60),
        "卷帘视图优先时应取卷帘选区（收口前是卷帘无条件优先，属巧合正确）"
    );
    // 走带优先 → 取走带选区（tick 5000）
    let arr = editor
        .resolve_selection(EditView::Arrangement)
        .into_tracks();
    assert_eq!(arr.len(), 1);
    assert_eq!(
        (arr[0].0, arr[0].1[0].start_tick, arr[0].1[0].key),
        (0, 5000, 67),
        "走带视图优先时应取走带选区"
    );
}

/// `has_active_selection` 与 `resolve_selection` 判空口径必须一致（闸门与菜单可用性同源）
#[test]
fn test_has_active_selection_matches_snapshot_emptiness() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 1, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
    for view in [EditView::PianoRoll, EditView::Arrangement] {
        assert!(
            !editor.has_active_selection(view),
            "{view:?}: 两套选区都空时闸门必须拦下"
        );
        // 走带框选落在空白处（结构非空但无命中音符）→ 仍应判不可用
        editor
            .editor_state
            .data
            .arrange_selection
            .add_rect_track(9000, 9010, 0, 127, 0, 0);
        assert!(
            !editor.has_active_selection(view),
            "{view:?}: 框选落在空白处时不得判为有可操作对象（否则「量化整轨」的坑会重现）"
        );
        editor.editor_state.data.arrange_selection.clear();
        // 真正命中
        editor
            .editor_state
            .data
            .arrange_selection
            .add_rect_track(0, 10, 0, 127, 0, 0);
        assert!(
            editor.has_active_selection(view),
            "{view:?}: 有命中音符时应放行"
        );
        editor.editor_state.data.arrange_selection.clear();
    }
}
