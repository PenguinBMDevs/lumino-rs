//! 曲线工具纵向卷帘回归测试：完整入口创建锚点 + 转置命中
//!
//! 背景 BUG：纵向卷帘下曲线工具"无法创建锚点和使用"。根因是渲染层
//! （`VerticalRollGrid::draw`）漏挂曲线工具图层 + `content_bounds`
//! 纯横向语义导致按钮定位失效；交互/数据层本就支持转置。
//! 本文件钉死交互层纵向语义，防止后续重构回退。

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;

/// 构造纵向卷帘编辑器（画布 800x600，键盘高度 120 → 网格区 y ∈ [0, 480)）
///
/// 默认视图：zoom_x=0.1、zoom_y=20、snap=1920、scroll=0。
fn vertical_curve_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    // 非 Conductor 音轨（track 1）以触发曲线工具真实交互（Conductor 整工具不可用）
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.is_vertical_roll = true;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor
}

/// 纵向卷帘完整入口：两次点击应创建完整路径，
/// 锚点逻辑坐标 = 点击位置的转置换算（tick 从 Y 吸附、key 从 X 取整）
#[test]
fn test_vertical_roll_pressed_creates_anchors() {
    let mut editor = vertical_curve_editor();
    // 点击 (200, 400)：tick=(480-400)/0.1=800→吸附 0；key=200/20=10
    let p1 = Point::new(200.0, 400.0);
    let expect_tick1 = editor.snap_tick(editor.pos_to_tick(p1));
    let expect_key1 = editor.pos_to_key(p1);
    editor.handle_pressed(p1, false);
    assert_eq!(
        editor.editor_state.line_tool.paths.len(),
        1,
        "第一次按下应创建路径"
    );
    assert_eq!(
        editor.editor_state.line_tool.paths[0][0].pos,
        (expect_tick1, expect_key1 as f32),
        "首锚点 = 点击位置转置换算（tick 吸附、key 取整）"
    );
    // 点击 (280, 140)：tick=(480-140)/0.1=3400→吸附 1920；key=280/20=14
    let p2 = Point::new(280.0, 140.0);
    let expect_tick2 = editor.snap_tick(editor.pos_to_tick(p2));
    let expect_key2 = editor.pos_to_key(p2);
    editor.handle_pressed(p2, false);
    let line = &editor.editor_state.line_tool;
    assert!(line.is_complete(), "第二次按下应完成路径");
    assert_eq!(
        line.paths[0][1].pos,
        (expect_tick2, expect_key2 as f32),
        "尾锚点走同一转置换算"
    );
    // 转置方向钉死：key 轴来自 X、tick 轴来自 Y（防轴互换回退）
    assert_eq!(expect_key1, 10, "key 应从屏幕 X 计算");
    assert_eq!(expect_tick1, 0.0, "tick 应从屏幕 Y 计算");
}

/// 回归 BUG：纵向卷帘下「垂直于时间轴的直线」（时间原地不动、一瞬间跨过多个音高行）
/// 生成的音符只有 1 tick —— 在 1920 ppq 下是 1/7680 个全音符，缩放到实机比例不足
/// 1px，用户看到的是「音符宽度过小 / 像没生成」。
///
/// 修复后：长度补齐到至少 128 分音符（= `ppq / 32`），档位由
/// `LineToolState::min_note_division` 提供修改接口。
#[test]
fn test_vertical_roll_straight_line_keeps_min_note_length() {
    let mut editor = vertical_curve_editor();
    let ppq = editor.editor_state.view.ppq;
    let min_ticks = (ppq / 32).max(1) as u32;
    {
        let line = &mut editor.editor_state.line_tool;
        // 纵向卷帘：屏幕竖直 = tick 轴 ⇒ 「垂直的直线」= 同一 tick 跨 9 个音高行
        line.paths.push(Vec::new());
        line.push_anchor(0, (1920.0, 60.0));
        line.push_anchor(0, (1920.0, 68.0));
    }
    assert!(editor.confirm_line_tool(), "纵向直线应生成音符");
    let notes = editor.editor_state.data.current_track_notes();
    assert!(!notes.is_empty(), "9 个音高行应各生成一条音符");
    for n in notes.iter() {
        let len = n.end_tick - n.start_tick;
        assert!(
            len >= min_ticks,
            "纵向直线生成的音符长度 {len} 必须 ≥ 128 分音符 = {min_ticks} tick \
             （key {} @ {})",
            n.key,
            n.start_tick
        );
    }
}

/// 纵向卷帘命中测试：锚点屏幕位置（转置）应命中 Anchor（拖动/删除的前提）
#[test]
fn test_vertical_roll_hit_test_finds_anchor() {
    let mut editor = vertical_curve_editor();
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (800.0, 10.0));
        line.push_anchor(0, (2400.0, 20.0));
        line.push_path_history();
    }
    // 纵向转置：x = key*zoom_y - scroll_y = 200；y = grid_bottom - tick*zoom_x = 400
    let sp = editor.line_pos_screen_pos((800.0, 10.0));
    assert!(
        (sp.x - 200.0).abs() < 0.5 && (sp.y - 400.0).abs() < 0.5,
        "纵向转置：key→X、tick→Y，实际 ({}, {})",
        sp.x,
        sp.y
    );
    let hit = editor.line_tool_hit_test(sp);
    assert!(
        matches!(hit, Some(LineToolHit::Anchor { path: 0, idx: 0 })),
        "纵向模式应命中首锚点，实际 {:?}",
        hit
    );
}
