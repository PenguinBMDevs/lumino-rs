//! 弯音锚点排序与跳变对连续拖动测试
//!
//! 覆盖：
//! - 吸附落回已有 tick 的锚点按 tick 有序插入（防渲染倒退）
//! - 跳变对低位锚点连续拖动：每次 Move 的 old_value 必须与 lane 当前值匹配

use iced_core::Point;
use iced_widget::canvas;
use lumino_core::Tool;
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use crate::velocity::widget;

use super::{anchor_screen, bend_canvas, move_cursor, press, velocity_action};

#[test]
fn test_bend_anchor_order_sorted_on_left_click() {
    // 回归：A(0) → B(1920) → 点击 B 下方偏左（吸附落回 tick 0）创建 C：
    // 锚点必须按 tick 有序插入（[A, C, B]），否则 B→C 段倒退渲染，
    // 连线视觉上"绕过中间锚点 B 直接连到 C"。
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 步骤 1：锚点 A（tick 0）
    press(&canvas, &mut state, Point::new(300.0, 100.0));
    // 步骤 2：A 右侧锚点 B（tick 1920）
    press(&canvas, &mut state, Point::new(500.0, 100.0));
    assert_eq!(state.bend_path.anchors.len(), 2);
    assert_eq!(state.bend_path.anchors[0].pos.0, 0.0);
    assert_eq!(state.bend_path.anchors[1].pos.0, 1920.0);

    // 步骤 3：B 下方偏左点击（raw tick 1300 → 吸附 0，与 A 同 tick）
    press(&canvas, &mut state, Point::new(250.0, 300.0));
    assert_eq!(state.bend_path.anchors.len(), 3, "应创建第三个锚点 C");

    // 有序不变式：tick 必须升序（C 插入到 A 之后，而不是 push 到末尾）
    let ticks: Vec<f32> = state.bend_path.anchors.iter().map(|a| a.pos.0).collect();
    assert_eq!(
        ticks,
        vec![0.0, 0.0, 1920.0],
        "锚点必须按 tick 有序: {ticks:?}"
    );
    assert!(
        ticks.windows(2).all(|w| w[0] <= w[1]),
        "锚点 tick 升序（乱序会致渲染倒退）: {ticks:?}"
    );
    // C（新锚点）在 A 之后：A(高) → C(低) 竖直段 → 下行跳变（创建顺序）
    assert_eq!(state.bend_path.anchors[1].pos.0, 0.0, "C 与 A 同 tick");
    assert!(
        state.bend_path.anchors[0].pos.1 > state.bend_path.anchors[1].pos.1,
        "A(先建,高) 在前、C(后建,低) 在后——跳变从 A 向下到 C"
    );
    assert_eq!(state.bend_path.anchors[2].pos.0, 1920.0, "B 保持位置");
    assert_eq!(state.bend_path.selected, Some(1), "新锚点 C 被选中");
}

#[test]
fn test_bend_drag_jump_pair_anchor_continuous() {
    // 回归：跳变对（同 tick 两锚点）中低位锚点连续拖动——每次 moved 的
    // Move 消息 old_value 必须与 lane 当前值匹配（用本地当前值）。
    // 旧实现用按下时原值：第一次 Move 后 lane 值已变，后续匹配失败
    // → 本地锚点跟着鼠标飞走、连线却不动。
    use lumino_note_core::SegmentShape;
    use lumino_note_core::automation::{AutomationEdit, AutomationTarget};

    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    // 预建 Bend lane（Move 消息需要定位 lane）
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

    // A → B → C（B 正下方，同 tick 1920 跳变对）
    press(&canvas, &mut state, Point::new(300.0, 100.0)); // A(0, 10922)
    press(&canvas, &mut state, Point::new(500.0, 100.0)); // B(1920, 10922)
    press(&canvas, &mut state, Point::new(312.0, 300.0)); // C(1920, 0)
    assert_eq!(state.bend_path.anchors.len(), 3);
    let c0 = state.bend_path.anchors[2].pos;

    // 点击 C 开始拖动
    let c_screen = anchor_screen(&editor, &state.bend_path.anchors[2]);
    press(&canvas, &mut state, c_screen);
    assert_eq!(
        state.bend_path.interaction,
        crate::velocity::widget::bend_path::BendInteraction::DraggingAnchor { idx: 2 }
    );

    // 连续拖动两次（每次都超过 4px 阈值）
    let move1 = move_cursor(
        &canvas,
        &mut state,
        Point::new(c_screen.x, c_screen.y - 50.0),
    );
    let move2 = move_cursor(
        &canvas,
        &mut state,
        Point::new(c_screen.x, c_screen.y - 100.0),
    );
    assert!(move1.is_some(), "第一次拖动应发 Move");
    assert!(move2.is_some(), "第二次拖动必须发 Move（旧实现此处失败）");

    // 本地锚点持续更新（跟着鼠标走）
    assert_ne!(state.bend_path.anchors[2].pos.1, c0.1, "本地 C 应更新");
    assert_eq!(state.bend_path.anchors[2].pos.0, c0.0, "tick 锁定");

    // 提取两次 Move 消息：Move#2 的 old_value 必须等于 Move#1 的 new_value
    // （本地当前值——与 lane 同步后仍能匹配），而不是按下时原值
    fn extract_move(action: canvas::Action<Message>) -> (u16, u16) {
        match velocity_action(action) {
            Some(VelocityAction::AutomationBatch(edits)) => match &edits[0] {
                lumino_note_core::automation::AutomationEdit::Move {
                    old_value,
                    new_value,
                    ..
                } => (old_value.unwrap_or(9999), *new_value),
                other => panic!("应发 Move，实际 {other:?}"),
            },
            other => panic!("应发 AutomationBatch，实际 {other:?}"),
        }
    }
    let (ov1, nv1) = extract_move(move1.expect("移动事件 1 应存在"));
    let (ov2, nv2) = extract_move(move2.expect("移动事件 2 应存在"));
    // Move#1: old_value = C 更新前值（原值 0），new_value = 拖到的新值
    assert_eq!(ov1, c0.1.round() as u16);
    assert_ne!(nv1, ov1, "Move#1 应更新 value");
    // 关键断言：Move#2 的 old_value == Move#1 的 new_value（连续匹配）
    assert_eq!(
        ov2, nv1,
        "Move#2 的 old_value 必须是 Move#1 后的当前值（旧实现传原值 0，匹配失败）"
    );
    assert_ne!(ov2, c0.1.round() as u16, "不得再使用按下时原值");
    assert_ne!(nv1, nv2, "两次拖动值不同（连续更新）");
}
