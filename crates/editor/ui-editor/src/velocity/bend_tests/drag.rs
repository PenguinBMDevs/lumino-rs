//! 弯音拖拽 / 残留交互状态重置测试
//!
//! 覆盖：
//! - 锚点创建后同一手势拖动不得移动/新增锚点
//! - 拖拽中鼠标移出面板（released 丢失）后，下次按下必须重置残留交互
//! - 已创建锚点只能调整 value（tick 锁定）、微动不改变高度

use iced_core::Point;
use lumino_core::Tool;
use lumino_ui_core::message::VelocityAction;

use crate::velocity::widget;
use crate::velocity::widget::bend_path::BendInteraction;

use super::{
    anchor_screen, bend_canvas, move_cursor, press, release, velocity_action, view_params,
};

#[test]
fn test_bend_created_anchor_cannot_drag() {
    // 回归：锚点创建后同一手势继续拖动，不得移动/新增锚点 ——
    // 锚点只能点击创建（创建即落定），创建后不跟随鼠标。
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 点击空白创建锚点（创建手势按下）
    let action = press(&canvas, &mut state, Point::new(300.0, 100.0));
    assert!(action.is_some());
    assert_eq!(state.bend_path.anchors.len(), 1);
    assert_eq!(
        state.bend_path.interaction,
        BendInteraction::None,
        "创建后不得进入拖拽状态"
    );
    let orig = state.bend_path.anchors[0].pos;

    // 同一手势继续拖动：锚点不得跟随鼠标
    move_cursor(&canvas, &mut state, Point::new(400.0, 150.0));
    assert_eq!(
        state.bend_path.anchors.len(),
        1,
        "创建手势的拖动不应新增锚点"
    );
    assert_eq!(
        state.bend_path.anchors[0].pos, orig,
        "创建手势的拖动不应移动锚点"
    );
    assert_eq!(state.bend_path.interaction, BendInteraction::None);

    // 松开后仍只有 1 个锚点
    release(&canvas, &mut state);
    assert_eq!(state.bend_path.anchors.len(), 1);
}

#[test]
fn test_bend_stale_interaction_reset_on_create() {
    // 回归：拖拽中鼠标移出面板（iced 不派发 released）导致 DraggingAnchor
    // 残留；下次按下创建锚点时必须重置，不得把新锚点拖走。
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 两个锚点形成线段
    press(&canvas, &mut state, Point::new(300.0, 100.0));
    press(&canvas, &mut state, Point::new(700.0, 100.0));
    assert_eq!(state.bend_path.anchors.len(), 2);
    let orig0 = state.bend_path.anchors[0].pos;

    // 模拟残留：拖动锚点 0 后 released 丢失
    state.bend_path.interaction = BendInteraction::DraggingAnchor { idx: 0 };

    // 点击线段中点：插入锚点（创建路径）
    let a0 = &state.bend_path.anchors[0];
    let a1 = &state.bend_path.anchors[1];
    let mid_tick = (a0.pos.0 + a1.pos.0) * 0.5;
    let mid_val = (a0.pos.1 + a1.pos.1) * 0.5;
    let v = view_params(&editor);
    let screen = Point::new(
        v.tick_to_x(mid_tick.round() as u32),
        v.value_to_y(mid_val, 16383.0),
    );
    let action = press(&canvas, &mut state, screen);
    assert!(action.is_some(), "点击线段应插入锚点");
    assert_eq!(state.bend_path.anchors.len(), 3, "应插入新锚点");
    assert_eq!(
        state.bend_path.interaction,
        BendInteraction::None,
        "残留交互必须被手势开始重置"
    );

    // 同一手势继续移动：任何锚点都不得被残留交互拖走
    move_cursor(&canvas, &mut state, Point::new(500.0, 200.0));
    assert_eq!(
        state.bend_path.anchors[0].pos, orig0,
        "残留拖拽不得移动锚点 0"
    );
    assert_eq!(state.bend_path.interaction, BendInteraction::None);
}

#[test]
fn test_bend_stale_interaction_reset_on_append() {
    // 残留交互 + 点击空白追加：同样不得拖动（覆盖另一条创建路径）
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    press(&canvas, &mut state, Point::new(300.0, 100.0));
    assert_eq!(state.bend_path.anchors.len(), 1);
    let orig0 = state.bend_path.anchors[0].pos;

    // 模拟残留：拖动锚点 0 后 released 丢失
    state.bend_path.interaction = BendInteraction::DraggingAnchor { idx: 0 };

    // 点击空白追加锚点
    press(&canvas, &mut state, Point::new(600.0, 150.0));
    assert_eq!(state.bend_path.anchors.len(), 2, "应追加新锚点");
    assert_eq!(
        state.bend_path.interaction,
        BendInteraction::None,
        "残留交互必须被手势开始重置"
    );

    // 继续移动：锚点 0 不得被拖走
    move_cursor(&canvas, &mut state, Point::new(650.0, 180.0));
    assert_eq!(
        state.bend_path.anchors[0].pos, orig0,
        "残留拖拽不得移动锚点 0"
    );
}

#[test]
fn test_bend_drag_anchor_locks_tick() {
    // 回归：已创建锚点不能被左右拖动——拖拽只调整 value，tick 锁定
    use lumino_note_core::SegmentShape;
    use lumino_note_core::automation::{AutomationEdit, AutomationTarget};

    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    // 预建 Bend lane（拖拽 Move 消息需要定位 lane）
    editor
        .editor_state
        .data
        .apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::PitchBend,
            channel: 0,
            tick: 0,
            value: 8192,
            shape: SegmentShape::Curve { tension: 0 },
        });
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    press(&canvas, &mut state, Point::new(300.0, 100.0));
    press(&canvas, &mut state, Point::new(500.0, 200.0));
    assert_eq!(state.bend_path.anchors.len(), 2);
    let tick0 = state.bend_path.anchors[0].pos.0;
    let tick1 = state.bend_path.anchors[1].pos.0;

    // 点击锚点 0 开始拖拽
    let screen = anchor_screen(&editor, &state.bend_path.anchors[0]);
    press(&canvas, &mut state, screen);
    assert_eq!(
        state.bend_path.interaction,
        BendInteraction::DraggingAnchor { idx: 0 }
    );

    // 向右下方拖动：tick 必须保持，value 更新
    let action = move_cursor(&canvas, &mut state, Point::new(800.0, 300.0));
    assert!(action.is_some(), "拖动应产生 Move 消息");
    assert_eq!(
        state.bend_path.anchors[0].pos.0, tick0,
        "锚点 0 tick 锁定（不能被左右拖动）"
    );
    assert_eq!(state.bend_path.anchors[1].pos.0, tick1, "锚点 1 不受影响");
    // value 已更新为点击处的 value
    let v = view_params(&editor);
    let expected_value = v.y_to_value(300.0, 16383.0).round().clamp(0.0, 16383.0);
    assert_eq!(state.bend_path.anchors[0].pos.1, expected_value);

    // Move 消息 tick 不变（new_tick == old_tick）
    let action = action.expect("应存在待处理动作");
    match velocity_action(action) {
        Some(VelocityAction::AutomationBatch(edits)) => {
            assert_eq!(edits.len(), 1);
            match &edits[0] {
                lumino_note_core::automation::AutomationEdit::Move {
                    old_tick,
                    new_tick,
                    new_value,
                    ..
                } => {
                    assert_eq!(old_tick, new_tick, "Move 不应改变 tick");
                    assert_eq!(*new_value, expected_value as u16);
                }
                other => panic!("应发 Move 消息，实际 {other:?}"),
            }
        }
        other => panic!("应发 AutomationBatch，实际 {other:?}"),
    }
}

#[test]
fn test_bend_click_on_anchor_does_not_change_height() {
    // 回归：点击锚点（移动距离 < 拖动阈值）只选中，不改变高度——
    // 锚点高度改变必须手动拖动
    use lumino_note_core::SegmentShape;
    use lumino_note_core::automation::{AutomationEdit, AutomationTarget};

    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    // 预建 Bend lane（拖动 Move 消息需要定位 lane）
    editor
        .editor_state
        .data
        .apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::PitchBend,
            channel: 0,
            tick: 0,
            value: 8192,
            shape: SegmentShape::Curve { tension: 0 },
        });
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    press(&canvas, &mut state, Point::new(300.0, 100.0));
    press(&canvas, &mut state, Point::new(500.0, 200.0));
    let orig_value = state.bend_path.anchors[0].pos.1;

    // 点击锚点 0（按下）
    let screen = anchor_screen(&editor, &state.bend_path.anchors[0]);
    press(&canvas, &mut state, screen);
    assert_eq!(
        state.bend_path.interaction,
        BendInteraction::DraggingAnchor { idx: 0 }
    );

    // 微动（< 4px 阈值）：不得改变高度、不得发消息
    let small_move = Point::new(screen.x + 2.0, screen.y + 1.0);
    let action = move_cursor(&canvas, &mut state, small_move);
    assert!(action.is_none(), "阈值内微动不应发消息");
    assert_eq!(
        state.bend_path.anchors[0].pos.1, orig_value,
        "阈值内微动不得改变高度"
    );

    // 超过阈值：进入真正拖动，高度改变
    let far_move = Point::new(screen.x, screen.y - 50.0);
    let action = move_cursor(&canvas, &mut state, far_move);
    assert!(action.is_some(), "超过阈值应发 Move 消息");
    assert_ne!(
        state.bend_path.anchors[0].pos.1, orig_value,
        "超过阈值拖动应改变高度"
    );

    // 松开后高度保持（拖动结果落定）
    release(&canvas, &mut state);
    assert_ne!(state.bend_path.anchors[0].pos.1, orig_value);
}
