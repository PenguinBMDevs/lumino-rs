//! 曲线工具贝塞尔路径测试

use super::geom::{curve_cell_points, point_curve_distance};
use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;
use lumino_editor_state::{BezierAnchor, HandleSide, LineToolInteraction};

// ── 离散化算法 ──

#[test]
fn test_curve_cell_points_degenerate_equals_line() {
    // 控制柄与锚点重合（直线退化）：结果与直线 Bresenham 一致
    let a = BezierAnchor::new((0.0, 60.0));
    let b = BezierAnchor::new((3840.0, 60.0));
    let pts = curve_cell_points(a, b, 1920.0);
    assert_eq!(pts, vec![(0.0, 60), (1920.0, 60), (3840.0, 60)]);
}

#[test]
fn test_curve_cell_points_vertical_no_gaps() {
    // 竖直段（tick 跨度 0）：采样由 key 跨度驱动，不漏格
    let a = BezierAnchor::new((1920.0, 60.0));
    let b = BezierAnchor::new((1920.0, 64.0));
    let pts = curve_cell_points(a, b, 1920.0);
    assert_eq!(pts.len(), 5);
    assert_eq!(pts[0], (1920.0, 60));
    assert_eq!(pts[4], (1920.0, 64));
}

#[test]
fn test_curve_cell_points_bent_curve() {
    // 弯曲曲线：出向柄拉高 → 曲线经过中间格点
    let mut a = BezierAnchor::new((0.0, 60.0));
    a.out_handle = (960.0, 20.0);
    let mut b = BezierAnchor::new((1920.0, 60.0));
    b.in_handle = (-960.0, 20.0);
    let pts = curve_cell_points(a, b, 1920.0);
    assert_eq!(pts.first(), Some(&(0.0, 60)));
    assert_eq!(pts.last(), Some(&(1920.0, 60)));
    // 中间应经过 key > 60 的格点（曲线拱起）
    assert!(
        pts.iter().any(|&(_, k)| k > 60),
        "弯曲曲线应经过更高 key 格点"
    );
}

#[test]
fn test_point_curve_distance() {
    // 直线退化：点到曲线距离 ≈ 点到线段距离
    let a = Point::new(0.0, 0.0);
    let p1 = Point::new(10.0, 0.0);
    let p2 = Point::new(20.0, 0.0);
    let b = Point::new(30.0, 0.0);
    let d = point_curve_distance(Point::new(15.0, 5.0), a, p1, p2, b);
    assert!((d - 5.0).abs() < 0.1, "直线退化距离应接近 5，实际 {d}");
}

// ── 交互流程 ──

#[test]
fn test_two_clicks_set_endpoints() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.handle_line_tool_pressed(Point::new(120.0, 24.0), 0.0, 60.0);
    assert_eq!(editor.editor_state.line_tool.anchors.len(), 1);
    assert!(!editor.editor_state.line_tool.is_complete());
    editor.handle_line_tool_pressed(Point::new(312.0, 24.0), 1920.0, 64.0);
    assert_eq!(editor.editor_state.line_tool.anchors.len(), 2);
    assert!(editor.editor_state.line_tool.is_complete());
    assert_eq!(editor.editor_state.line_tool.anchors[1].pos, (1920.0, 64.0));
}

#[test]
fn test_press_segment_release_inserts_anchor() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((3840.0, 60.0));
    }
    // 段 0 中点屏幕位置（tick 1920, key 60）
    let mid = editor.line_pos_screen_pos((1920.0, 60.0));
    editor.handle_line_tool_pressed(mid, 1920.0, 60.0);
    assert_eq!(
        editor.editor_state.line_tool.interaction,
        LineToolInteraction::DraggingLine { segment: 0 }
    );
    assert!(!editor.editor_state.line_tool.drag_confirmed);
    // 原地松开（无移动）→ 插入锚点，位置 = 按下处（不吸附）
    editor.handle_line_tool_released();
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.anchors.len(), 3);
    assert_eq!(line.anchors[1].pos, (1920.0, 60.0));
    assert_eq!(line.interaction, LineToolInteraction::None);
}

#[test]
fn test_drag_segment_translates_path() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((1920.0, 64.0));
    }
    let mid = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_pressed(mid, 0.0, 60.0);
    assert!(!editor.editor_state.line_tool.drag_confirmed);
    // 大幅移动 → 确认拖动并平移（snap 增量）
    editor.handle_line_tool_moved(1920.0, 60.0, 2000.0, 62.0);
    assert!(editor.editor_state.line_tool.drag_confirmed);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.anchors[0].pos, (1920.0, 60.0));
    assert_eq!(line.anchors[1].pos, (3840.0, 64.0));
    // 已确认拖动 → 松开不插入
    let mut editor = editor;
    editor.handle_line_tool_released();
    assert_eq!(editor.editor_state.line_tool.anchors.len(), 2);
}

#[test]
fn test_drag_endpoint_anchor_snaps() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((1920.0, 64.0));
    }
    let a_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_pressed(a_pos, 0.0, 60.0);
    // 端点拖动：snap 增量（即使 raw 更远也吸附）
    editor.handle_line_tool_moved(1920.0, 64.0, 2000.0, 70.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.anchors[0].pos, (1920.0, 64.0));
    assert_eq!(line.anchors[1].pos, (1920.0, 64.0), "终点锚点不动");
}

#[test]
fn test_drag_middle_anchor_free() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((960.0, 62.0));
        line.push_anchor((1920.0, 64.0));
    }
    let m_pos = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_pressed(m_pos, 960.0, 62.0);
    // 中间锚点：raw 增量（自由精确定位，不吸附）
    editor.handle_line_tool_moved(960.0, 62.0, 960.5, 62.25);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.anchors[1].pos, (960.5, 62.25));
    assert_eq!(line.anchors[0].pos, (0.0, 60.0), "其他锚点不动");
}

#[test]
fn test_drag_handle_curves() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((1920.0, 64.0));
        // 先拖开柄（模拟已弯曲）：柄不再与锚点重合，参与命中
        line.anchors[0].out_handle = (300.0, -30.0);
    }
    // 柄屏幕位置（绝对坐标 300, 30）
    let h_pos = editor.line_pos_screen_pos((300.0, 30.0));
    editor.handle_line_tool_pressed(h_pos, 0.0, 60.0);
    assert!(matches!(
        editor.editor_state.line_tool.interaction,
        LineToolInteraction::DraggingHandle {
            anchor_idx: 0,
            side: HandleSide::Out
        }
    ));
    // 继续拖动柄：偏移增量（raw）
    editor.handle_line_tool_moved(0.0, 60.0, 400.0, 20.0);
    assert_eq!(
        editor.editor_state.line_tool.anchors[0].out_handle,
        (400.0, -40.0)
    );
    // 锚点位置不受柄拖动影响
    assert_eq!(editor.editor_state.line_tool.anchors[0].pos, (0.0, 60.0));
}

#[test]
fn test_handle_coincident_with_anchor_prefers_anchor() {
    // 柄与锚点重合（未弯曲）时：点击拖动的是锚点而非柄
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((1920.0, 64.0));
    }
    let a_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_pressed(a_pos, 0.0, 60.0);
    assert_eq!(
        editor.editor_state.line_tool.interaction,
        LineToolInteraction::DraggingAnchor(0)
    );
}

#[test]
fn test_press_blank_restarts() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((1920.0, 64.0));
    }
    // 远处空白按下 → 清空旧路径并从该点开始
    editor.handle_line_tool_pressed(Point::new(800.0, 500.0), 1920.0, 30.0);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.anchors.len(), 1);
    assert_eq!(line.anchors[0].pos, (1920.0, 30.0));
}

#[test]
fn test_double_click_deletes_middle_anchor() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((960.0, 62.0));
        line.push_anchor((1920.0, 64.0));
    }
    let m_pos = editor.line_pos_screen_pos((960.0, 62.0));
    editor.handle_line_tool_double_clicked(m_pos);
    let line = &editor.editor_state.line_tool;
    assert_eq!(line.anchors.len(), 2);
    assert_eq!(line.anchors[0].pos, (0.0, 60.0));
    assert_eq!(line.anchors[1].pos, (1920.0, 64.0));
}

#[test]
fn test_double_click_endpoint_kept() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((960.0, 62.0));
        line.push_anchor((1920.0, 64.0));
    }
    // 双击端点：不删除
    let e_pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_line_tool_double_clicked(e_pos);
    assert_eq!(editor.editor_state.line_tool.anchors.len(), 3);
}

// ── 确认生成 ──

#[test]
fn test_confirm_line_creates_notes() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 1, 0, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((1920.0, 64.0));
    }
    assert!(editor.confirm_line_tool());
    // 45° 直线退化：5 个格点 → 5 个音符
    assert_eq!(editor.editor_state.data.current_track_note_count(), 5);
    // 路径状态已清空
    assert!(editor.editor_state.line_tool.anchors.is_empty());
}

#[test]
fn test_confirm_bent_curve_creates_more_notes() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    seed_notes(&mut editor, 1, 0, &[]);
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((1920.0, 64.0));
        // 插入中间锚点并弯曲
        line.insert_anchor_at(1, (960.0, 62.0));
        line.anchors[1].out_handle = (480.0, 12.0);
        line.anchors[1].in_handle = (-480.0, 12.0);
    }
    assert!(editor.confirm_line_tool());
    // 弯曲路径覆盖更多格点（> 5）
    let count = editor.editor_state.data.current_track_note_count();
    assert!(count > 5, "弯曲曲线应生成更多音符，实际 {count}");
}

#[test]
fn test_confirm_line_incomplete_noop() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
    }
    assert!(!editor.confirm_line_tool());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    assert_eq!(
        editor.editor_state.line_tool.anchors.len(),
        1,
        "未完整不改变状态"
    );
}

#[test]
fn test_cancel_line_clears() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    {
        let line = &mut editor.editor_state.line_tool;
        line.push_anchor((0.0, 60.0));
        line.push_anchor((1920.0, 64.0));
    }
    editor.cancel_line_tool();
    assert!(editor.editor_state.line_tool.anchors.is_empty());
}
