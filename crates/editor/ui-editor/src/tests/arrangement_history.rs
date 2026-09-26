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
