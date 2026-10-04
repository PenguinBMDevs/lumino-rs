//! 曲线工具批量确认/取消测试：多条路径一次 √ 全部生成、× 全部取消
//!
//! 生成语义为蜘蛛网（Spiderweb）式：每条**音高行一条音符**、两两无缝连奏、
//! 长度由曲线与行边界的解析交点决定——因此断言的是**音符覆盖的几何**
//! （哪条音高行在哪个 tick 区间被覆盖），而不是"格点数量"。

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;
use lumino_midi_model::{ChunkedList, NoteEvent};

/// 构造曲线工具 + 一条完整路径 (0,60)-(3840,60) 的编辑器
///
/// 路径记录为历史基准（模拟正常交互创建后的状态），后续操作
/// undo 恢复到该基准而非空状态。
fn line_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (3840.0, 60.0));
        line.push_path_history();
    }
    editor
}

/// 音符集合里是否有覆盖 (tick, key) 的一条音符
fn covers(notes: &ChunkedList<NoteEvent>, tick: u32, key: u16) -> bool {
    notes
        .iter()
        .any(|n| n.key == key as u8 && n.start_tick <= tick && n.end_tick > tick)
}

// ── 确认生成（批量） ──

#[test]
fn test_confirm_line_creates_notes() {
    let mut editor = line_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    assert!(editor.confirm_line_tool());
    // 水平线只经过一个音高行 → 恰好一条音符铺满整条线（不是 N 个定长格子）
    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
    assert!(editor.editor_state.line_tool.paths.is_empty(), "确认后清空");
}

#[test]
fn test_confirm_last_anchor_aligns_note_tail() {
    // 最后一个锚点对齐最后一个音符**尾部**（曲线终点之外不得多出音符）
    let mut editor = line_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    assert_eq!(notes.len(), 1, "水平线 (0,60)-(3840,60) 只有一条音符");
    let n = notes.iter().next().expect("确认后应有音符");
    assert_eq!(n.start_tick, 0, "音符起于起点锚点");
    assert_eq!(n.end_tick, 3840, "音符尾部对齐终点锚点");
}

#[test]
fn test_confirm_diagonal_gives_one_note_per_row_seamless() {
    // key 60 → 64 的直线：5 条音高行各分到等长的一份时间、两两无缝
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 2, 1, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (3840.0, 64.0));
    }
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    assert_eq!(notes.len(), 5, "5 条音高行 → 5 条音符");
    for (i, key) in (60u16..=64).enumerate() {
        let lo = 768 * i as u32;
        let found = notes
            .iter()
            .find(|n| n.key == key as u8)
            .unwrap_or_else(|| panic!("音高行 {key} 应有音符"));
        assert_eq!(found.start_tick, lo, "音高行 {key} 起点");
        assert_eq!(found.end_tick, lo + 768, "音高行 {key} 终点（无缝衔接）");
    }
}

#[test]
fn test_confirm_multiple_paths_batch() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 2, 1, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        // 路径 1：水平线 (0,60)-(3840,60) → 1 条音符
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (3840.0, 60.0));
        // 路径 2：竖直线 (3840,64)-(3840,68) → 5 条音高行，
        // 各 1 tick 的竖直段音符按最小长度下限补齐到 128 分音符
        line.paths.push(Vec::new());
        line.push_anchor(1, (3840.0, 64.0));
        line.push_anchor(1, (3840.0, 68.0));
    }
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    assert_eq!(notes.len(), 6, "1（水平线）+ 5（竖直段逐行）");
    assert!(
        notes
            .iter()
            .any(|n| n.key == 60 && n.start_tick == 0 && n.end_tick == 3840),
        "水平线铺满"
    );
    for key in 64u16..=68 {
        assert!(covers(notes, 3840, key), "竖直段经过音高行 {key}");
    }
    assert!(editor.editor_state.line_tool.paths.is_empty());
}

#[test]
fn test_confirm_incomplete_paths_skipped() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 2, 1, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        // 完整路径
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (1920.0, 60.0));
        // 未完整路径（单锚点，应被跳过）
        line.paths
            .push(vec![lumino_editor_state::BezierAnchor::new((0.0, 70.0))]);
    }
    assert!(editor.confirm_line_tool());
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        1,
        "单锚点路径被跳过，只生成完整路径的音符"
    );
    assert!(editor.editor_state.line_tool.paths.is_empty());
}

#[test]
fn test_confirm_line_incomplete_noop() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths
            .push(vec![lumino_editor_state::BezierAnchor::new((0.0, 60.0))]);
    }
    assert!(!editor.confirm_line_tool());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    assert_eq!(
        editor.editor_state.line_tool.paths.len(),
        1,
        "未完整不改变状态"
    );
}

#[test]
fn test_cancel_line_clears() {
    let mut editor = line_editor();
    editor.cancel_line_tool();
    assert!(editor.editor_state.line_tool.paths.is_empty());
    assert!(
        !editor.editor_state.line_tool.can_undo_path(),
        "取消应清空历史"
    );
}

#[test]
fn test_confirm_line_rejected_on_conductor_track() {
    // 回归：曲线工具不得在 Conductor 音轨（track 0）放置音符，
    // 与铅笔 finish_drawing、文字工具 confirm_text_tool 的 `current_track == 0` 守卫一致。
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    // 选中 Conductor 音轨（track 0），预置一条完整路径（普通轨会生成音符）
    seed_notes(&mut editor, 1, 0, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (480.0, 60.0));
    }

    assert!(
        !editor.confirm_line_tool(),
        "Conductor 音轨（track 0）禁止放置音符：确认必须返回 false"
    );
    assert_eq!(
        editor.editor_state.data.track_notes(0).len(),
        0,
        "Conductor 音轨不应写入任何音符"
    );
}

#[test]
fn test_confirm_keeps_longest_when_outline_and_fill_overlap() {
    // 同一 (tick, key) 上出现多条时只留最长的一条 —— 由 `paths::keep_longest`
    // 保证（轮廓与填充大量重叠时靠它合并）
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 2, 1, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        // 长水平线 → (0, 3840, 60)
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (3840.0, 60.0));
        // 短水平线：同一起点、同一音高行 → (0, 960, 60)，被上面那条吞掉
        line.paths.push(Vec::new());
        line.push_anchor(1, (0.0, 60.0));
        line.push_anchor(1, (960.0, 60.0));
    }
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    assert_eq!(notes.len(), 1, "同 tick 同 key 只留最长的一条");
    let n = notes.iter().next().expect("应有音符");
    assert_eq!((n.start_tick, n.end_tick), (0, 3840), "保留最长的那条");
}

// ── 生成音符的**最小长度下限**（BUG：竖直段 1 tick 缩放下不足 1px）──────────

/// 构造曲线工具 + 一条竖直直线 (1920, 60)-(1920, 68)（同一 tick 跨 9 个音高行）
fn vertical_line_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 2, 1, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (1920.0, 60.0));
        line.push_anchor(0, (1920.0, 68.0));
    }
    editor
}

/// 回归 BUG：竖直线（时间原地不动、一瞬间跨多行）除最后一条外都只占 1 tick ——
/// 1920 ppq 下 1/7680 个全音符，实机缩放下不足 1px（「音符宽度过小」）。
/// 修复后：按最小长度下限（默认 128 分音符 = `ppq / 32`）补齐。
#[test]
fn test_confirm_vertical_line_keeps_min_note_length() {
    let mut editor = vertical_line_editor();
    let min_ticks = (editor.editor_state.view.ppq / 32).max(1) as u32;
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    assert_eq!(notes.len(), 9, "9 个音高行各一条音符");
    for n in notes.iter() {
        assert_eq!(
            n.end_tick - n.start_tick,
            min_ticks,
            "key {} 的竖直段音符应补齐到 128 分音符 = {min_ticks} tick",
            n.key
        );
    }
    // 起点不动（仍起于几何交点 1920），只向外扩终点
    assert!(notes.iter().all(|n| n.start_tick == 1920));
}

/// 修改接口：`min_note_division` 按 x 分音符换算下限；`None` = 不设下限（几何原样）。
#[test]
fn test_min_note_division_is_configurable() {
    // 六十四分音符档 = ppq / 16（默认 128 分音符档的两倍长）
    let mut editor = vertical_line_editor();
    editor.set_min_note_division(Some(64));
    let expected = (editor.editor_state.view.ppq / 16).max(1) as u32;
    assert!(editor.confirm_line_tool());
    assert!(
        editor
            .editor_state
            .data
            .current_track_notes()
            .iter()
            .all(|n| n.end_tick - n.start_tick == expected),
        "64 分音符档 = 4·ppq/64 = ppq/16 = {expected} tick"
    );

    // 关闭下限：竖直段保持 1 tick（几何原样，用于需要精确几何的调用方）
    let mut editor = vertical_line_editor();
    assert_eq!(editor.min_note_division(), Some(128), "默认 128 分音符档");
    editor.set_min_note_division(None);
    assert_eq!(editor.min_note_division(), None);
    assert!(editor.confirm_line_tool());
    assert!(
        editor
            .editor_state
            .data
            .current_track_notes()
            .iter()
            .all(|n| n.end_tick - n.start_tick == 1),
        "关闭下限后竖直段保持 1 tick"
    );

    // `Some(0)` 归一为 `None`（0 分音符无意义）
    editor.set_min_note_division(Some(0));
    assert_eq!(editor.min_note_division(), None);
}

/// 档位是模式设置：√ 确认（`reset`）后保留，不必每次重设
#[test]
fn test_min_note_division_survives_confirm() {
    let mut editor = vertical_line_editor();
    editor.set_min_note_division(Some(32));
    assert!(editor.confirm_line_tool());
    assert_eq!(editor.min_note_division(), Some(32), "√ 确认后档位保留");
}
