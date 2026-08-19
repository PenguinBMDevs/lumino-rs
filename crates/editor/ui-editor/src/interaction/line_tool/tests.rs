//! 曲线工具贝塞尔路径测试：多路径交互 + 编辑历史 + 批量生成

use super::*;
use lumino_core::Tool;
use lumino_editor_state::{HandleSide, LineToolInteraction};

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

// ── 创建与多路径 ──

#[test]
fn test_two_clicks_set_endpoints() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.handle_line_tool_pressed(Point::new(120.0, 24.0), 0.0, 60.0);
    assert_eq!(editor.editor_state.line_tool.paths.len(), 1);
    assert_eq!(editor.editor_state.line_tool.paths[0].len(), 1);
    editor.handle_line_tool_pressed(Point::new(312.0, 24.0), 1920.0, 64.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths[0].len(), 2);
    assert!(line.is_complete());
    assert_eq!(line.paths[0][1].pos, (1920.0, 64.0));
}

#[test]
fn test_blank_press_starts_new_path_keeps_existing() {
    let mut editor = line_editor();
    // 空白处按下：新建路径（不清空已有）
    editor.handle_line_tool_pressed(Point::new(800.0, 500.0), 1920.0, 30.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths.len(), 2, "已有路径应保留");
    assert_eq!(line.paths[0].len(), 2);
    assert_eq!(line.paths[1].len(), 1);
    assert_eq!(line.paths[1][0].pos, (1920.0, 30.0));
}

// ── 段点击插入 / 平移 ──

#[test]
fn test_press_segment_release_inserts_anchor() {
    let mut editor = line_editor();
    let mid = editor.line_pos_screen_pos((1920.0, 60.0));
    editor.handle_line_tool_pressed(mid, 1920.0, 60.0);
    assert_eq!(
        editor.editor_state.line_tool.interaction,
        LineToolInteraction::DraggingLine {
            path: 0,
            segment: 0
        }
    );
    assert!(!editor.editor_state.line_tool.drag_confirmed);
    editor.handle_line_tool_released();
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths[0].len(), 3);
    assert_eq!(line.paths[0][1].pos, (1920.0, 60.0));
    assert_eq!(line.interaction, LineToolInteraction::None);
}

#[test]
fn test_drag_segment_translates_path() {
    let mut editor = line_editor();
    let mid = editor.line_pos_screen_pos((1920.0, 60.0));
    editor.handle_line_tool_pressed(mid, 0.0, 60.0);
    editor.handle_line_tool_moved(1920.0, 60.0, 2000.0, 62.0);
    assert!(editor.editor_state.line_tool.drag_confirmed);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths[0][0].pos, (1920.0, 60.0));
    assert_eq!(line.paths[0][1].pos, (5760.0, 60.0));
    editor.handle_line_tool_released();
    assert_eq!(editor.editor_state.line_tool.paths[0].len(), 2);
}

// ── 锚点拖动 ──

#[test]
fn test_drag_endpoint_anchor_snaps() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (1920.0, 64.0));
    }
    let a_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_pressed(a_pos, 0.0, 60.0);
    editor.handle_line_tool_moved(1920.0, 64.0, 2000.0, 70.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths[0][0].pos, (1920.0, 64.0), "端点 snap 拖动");
    assert_eq!(line.paths[0][1].pos, (1920.0, 64.0), "终点锚点不动");
}

#[test]
fn test_drag_middle_anchor_free() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (960.0, 62.0));
        line.push_anchor(0, (1920.0, 64.0));
    }
    let m_pos = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_pressed(m_pos, 960.0, 62.0);
    editor.handle_line_tool_moved(960.0, 62.0, 960.5, 62.25);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths[0][1].pos, (960.5, 62.25), "中间锚点自由移动");
    assert_eq!(line.paths[0][0].pos, (0.0, 60.0));
}

// ── 锚点拖动磁吸（跨路径对齐） ──

/// 两条路径：P0 (0,60)-(960,62)-(1920,64)，P1 (3840,64)-(4800,64)
fn magnet_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (960.0, 62.0));
        line.push_anchor(0, (1920.0, 64.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (3840.0, 64.0));
        line.push_anchor(1, (4800.0, 64.0));
    }
    editor
}

#[test]
fn test_drag_anchor_magnets_to_other_path() {
    let mut editor = magnet_editor();
    // 拖动 P0 中间锚点 (960,62) 到 P1 起点 (3840,64)（跨路径）→ 磁吸
    let m_pos = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_pressed(m_pos, 960.0, 62.0);
    editor.handle_line_tool_moved(0.0, 0.0, 3840.0, 64.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths[0][1].pos, (3840.0, 64.0), "跨路径锚点磁吸");
}

#[test]
fn test_drag_anchor_same_path_no_magnet() {
    let mut editor = magnet_editor();
    // 拖动 P0 中间锚点靠近同路径终点 (1920,64)（屏幕 <16px）→ 不磁吸
    let zoom_x = editor.editor_state.view.zoom_x;
    let target_tick = 1920.0 - 5.0 / zoom_x; // 距终点 5px
    let m_pos = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_pressed(m_pos, 960.0, 62.0);
    editor.handle_line_tool_moved(0.0, 0.0, target_tick, 64.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(
        line.paths[0][1].pos.0, target_tick,
        "同路径锚点不参与磁吸，停在目标位置"
    );
    assert_eq!(line.paths[0][1].pos.1, 64.0);
}

#[test]
fn test_drag_anchor_magnet_outside_threshold_free() {
    let mut editor = magnet_editor();
    // 目标距 P1 起点 (3840,64) 屏幕 > 16px → 不磁吸
    let zoom_x = editor.editor_state.view.zoom_x;
    let target_tick = 3840.0 + 20.0 / zoom_x; // 距 20px
    let m_pos = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_pressed(m_pos, 960.0, 62.0);
    editor.handle_line_tool_moved(0.0, 0.0, target_tick, 64.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths[0][1].pos.0, target_tick, "超出阈值自由移动");
}

// ── 控制柄 ──

#[test]
fn test_drag_handle_curves() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (1920.0, 64.0));
        line.paths[0][0].set_out_handle((300.0, -30.0));
    }
    let h_pos = editor.line_pos_screen_pos((300.0, 30.0));
    editor.handle_line_tool_pressed(h_pos, 0.0, 60.0);
    assert!(matches!(
        editor.editor_state.line_tool.interaction,
        LineToolInteraction::DraggingHandle {
            path: 0,
            anchor_idx: 0,
            side: HandleSide::Out
        }
    ));
    editor.handle_line_tool_moved(0.0, 60.0, 400.0, 20.0);
    assert_eq!(
        editor.editor_state.line_tool.paths[0][0].out_handle,
        (400.0, -40.0)
    );
    assert_eq!(editor.editor_state.line_tool.paths[0][0].pos, (0.0, 60.0));
}

#[test]
fn test_handle_coincident_with_anchor_prefers_anchor() {
    let mut editor = line_editor();
    let a_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_pressed(a_pos, 0.0, 60.0);
    assert_eq!(
        editor.editor_state.line_tool.interaction,
        LineToolInteraction::DraggingAnchor { path: 0, idx: 0 }
    );
}

// ── 双击删除 ──

#[test]
fn test_double_click_deletes_middle_anchor() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (960.0, 62.0));
        line.push_anchor(0, (1920.0, 64.0));
    }
    let m_pos = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_double_clicked(m_pos);
    assert_eq!(editor.editor_state.line_tool.paths[0].len(), 2);
}

#[test]
fn test_double_click_endpoint_kept() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (960.0, 62.0));
        line.push_anchor(0, (1920.0, 64.0));
    }
    let e_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_double_clicked(e_pos);
    assert_eq!(editor.editor_state.line_tool.paths[0].len(), 3);
}

// ── 锚点吸附 ──

#[test]
fn test_set_endpoint_snaps_to_existing_anchor() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.handle_line_tool_pressed(Point::new(120.0, 24.0), 0.0, 60.0);
    let a_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_pressed(Point::new(a_pos.x + 5.0, a_pos.y), 1920.0, 64.0);
    assert_eq!(
        editor.editor_state.line_tool.paths[0][1].pos,
        (0.0, 60.0),
        "第二端点应吸附到起点锚点"
    );
}

#[test]
fn test_insert_anchor_snaps_to_nearby_anchor() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (3840.0, 60.0));
        line.push_anchor(0, (1920.0, 60.7));
    }
    // 点击段 [0,1] 上、C 正下方 14px（锚点命中半径外、吸附阈值内）
    let click = Point::new(312.0, 1364.0);
    editor.handle_line_tool_pressed(click, 1920.0, 60.0);
    editor.handle_line_tool_released();
    assert_eq!(
        editor.editor_state.line_tool.paths[0][1].pos,
        (1920.0, 60.7),
        "新锚点应吸附到附近锚点"
    );
}

// ── 编辑历史（撤销/重做） ──

#[test]
fn test_undo_removes_created_path() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    // 创建曲线（两次点击，合并为一次历史）
    editor.handle_line_tool_pressed(Point::new(120.0, 24.0), 0.0, 60.0);
    editor.handle_line_tool_pressed(Point::new(312.0, 24.0), 1920.0, 64.0);
    assert_eq!(editor.editor_state.line_tool.paths.len(), 1);
    // 一次撤销删除整条曲线
    assert!(editor.undo());
    assert!(
        editor.editor_state.line_tool.paths.is_empty(),
        "撤销应删除整条曲线"
    );
}

#[test]
fn test_redo_restores_created_path() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.handle_line_tool_pressed(Point::new(120.0, 24.0), 0.0, 60.0);
    editor.handle_line_tool_pressed(Point::new(312.0, 24.0), 1920.0, 64.0);
    editor.undo();
    assert!(editor.redo());
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.paths.len(), 1);
    assert_eq!(line.paths[0].len(), 2);
}

#[test]
fn test_undo_after_drag_restores_shape() {
    let mut editor = line_editor();
    let a_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_pressed(a_pos, 0.0, 60.0);
    editor.handle_line_tool_moved(1920.0, 64.0, 2000.0, 70.0);
    editor.handle_line_tool_released();
    assert_eq!(
        editor.editor_state.line_tool.paths[0][0].pos,
        (1920.0, 64.0)
    );
    // 撤销拖动：锚点回到原位
    assert!(editor.undo());
    assert_eq!(editor.editor_state.line_tool.paths[0][0].pos, (0.0, 60.0));
}

#[test]
fn test_undo_after_insert_anchor() {
    let mut editor = line_editor();
    let mid = editor.line_pos_screen_pos((1920.0, 60.0));
    editor.handle_line_tool_pressed(mid, 1920.0, 60.0);
    editor.handle_line_tool_released();
    assert_eq!(editor.editor_state.line_tool.paths[0].len(), 3);
    // 撤销插入：锚点消失
    assert!(editor.undo());
    assert_eq!(editor.editor_state.line_tool.paths[0].len(), 2);
}

#[test]
fn test_undo_after_delete_anchor() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (960.0, 62.0));
        line.push_anchor(0, (1920.0, 64.0));
        line.push_path_history(); // 记录基准状态
    }
    let m_pos = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_double_clicked(m_pos);
    assert_eq!(editor.editor_state.line_tool.paths[0].len(), 2);
    // 撤销删除：锚点恢复
    assert!(editor.undo());
    assert_eq!(editor.editor_state.line_tool.paths[0].len(), 3);
    assert_eq!(editor.editor_state.line_tool.paths[0][1].pos, (960.0, 62.0));
}

#[test]
fn test_press_no_drag_pops_empty_history() {
    let mut editor = line_editor();
    let before = editor.editor_state.line_tool.path_history_index;
    let a_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_pressed(a_pos, 0.0, 60.0);
    // 按下未拖动直接松开：不产生新历史
    editor.handle_line_tool_released();
    assert_eq!(
        editor.editor_state.line_tool.path_history_index, before,
        "空操作不应产生撤销历史"
    );
}
