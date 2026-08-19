//! 曲线工具批量确认/取消测试：多条路径一次 √ 全部生成、× 全部取消

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;

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

// ── 确认生成（批量） ──

#[test]
fn test_confirm_line_creates_notes() {
    let mut editor = line_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    assert!(editor.confirm_line_tool());
    // 水平线 (0,60)-(3840,60)：终点锚点对齐最后一个音符尾部 →
    // 格点 (0,1920,3840) 中终点格点 -snap 与相邻格点去重 → 2 个音符
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);
    assert!(editor.editor_state.line_tool.paths.is_empty(), "确认后清空");
}

#[test]
fn test_confirm_last_anchor_aligns_note_tail() {
    // 最后一个锚点对齐最后一个音符**尾部**（原行为头部对齐：曲线终点
    // 处会多出 [tick, tick+snap) 半格音符，尾部超出曲线终点）
    let mut editor = line_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    assert_eq!(notes.len(), 2, "曲线 (0,60)-(3840,60) 铺满 2 个音符");
    let max_end = notes
        .iter()
        .map(|n| n.end_tick)
        .max()
        .expect("确认后应有音符");
    assert_eq!(max_end, 3840, "最后一个音符尾部对齐终点锚点");
    let max_start = notes
        .iter()
        .map(|n| n.start_tick)
        .max()
        .expect("确认后应有音符");
    assert_eq!(max_start, 1920, "最后一个音符头部 = 锚点 - snap");
}

#[test]
fn test_confirm_multiple_paths_batch() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 1, 0, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        // 路径 1：水平 3 格
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (3840.0, 60.0));
        // 路径 2：垂直 5 格（tick 相同）
        line.paths.push(Vec::new());
        line.push_anchor(1, (3840.0, 64.0));
        line.push_anchor(1, (3840.0, 68.0));
    }
    assert!(editor.confirm_line_tool());
    // 路径 1 水平 3 格 → 终点去重后 2；路径 2 垂直 5 格 → 终点 -snap 新增 1
    // → 总 2 + 5 = 7 个音符
    assert_eq!(editor.editor_state.data.current_track_note_count(), 7);
    assert!(editor.editor_state.line_tool.paths.is_empty());
}

#[test]
fn test_confirm_incomplete_paths_skipped() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 1, 0, &[]);
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
    // 路径 (0,60)-(1920,60)：2 格点 → 终点 -snap 与起点去重 → 1 个音符
    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
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
