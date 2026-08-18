//! 弯音同 tick 跳变对创建/约束测试
//!
//! 覆盖：
//! - 同高度（吸附后完全重合）点击创建第二个锚点（跳变对初始状态）
//! - 同 tick 最多 2 个锚点（第 3 个被拒绝）
//! - 同 tick 不同 value 合法（竖直跳变段）、竖直段点击不得插入乱序锚点
//! - 向下跳变保持 X 位置不动

use iced_core::Point;
use lumino_core::Tool;

use crate::velocity::widget;

use super::{anchor_screen, bend_canvas, move_cursor, press, view_params};

#[test]
fn test_bend_same_height_click_creates_jump_pair() {
    // 回归：同一 Y 高度（吸附后与已有锚点完全重合）点击必须创建第二个
    // 锚点（跳变对的初始状态，与第一个重叠显示）——随后拖动分离形成
    // 上行的突变。旧实现直接放弃创建（anchor_at_pos 拒绝）。
    // 默认网格 = 四分音符 1920 ticks、zoom_x=0.1、keyboard_width=120：
    // 点击 (300,100) → tick 1800 → 吸附 0，value 10922；
    // 点击 (250,100) → tick 1300 → 吸附仍 0，value 相同。
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let mut state = widget::VelocityCanvasState::default();

    {
        let canvas = bend_canvas(&editor);
        press(&canvas, &mut state, Point::new(300.0, 100.0));
        assert_eq!(state.bend_path.anchors.len(), 1);
        assert_eq!(state.bend_path.anchors[0].pos, (0.0, 10922.0));

        // 同高度点击（吸附后完全重合）：必须创建第二个锚点
        let action = press(&canvas, &mut state, Point::new(250.0, 100.0));
        assert!(action.is_some(), "重合锚点创建应发 Add 消息");
        assert_eq!(
            state.bend_path.anchors.len(),
            2,
            "同高度点击必须创建第二个锚点（不能放弃）"
        );
        assert_eq!(
            state.bend_path.anchors[1].pos,
            (0.0, 10922.0),
            "第二个锚点初始与第一个重合（跳变对初始状态）"
        );
        assert_eq!(state.bend_path.selected, Some(1), "新锚点被选中");

        // 同 tick 上限：第三个同 tick 锚点被拒绝（与重合创建不冲突）
        press(&canvas, &mut state, Point::new(200.0, 100.0));
        assert_eq!(
            state.bend_path.anchors.len(),
            2,
            "同 tick 超过 2 个仍被拒绝"
        );
    }

    // 拖动锚点上行（y 减小 = value 增大）→ 分离形成上行突变。
    // 重合状态下点击命中第一个锚点（hit test 顺序），拖动它分离即可。
    use lumino_note_core::SegmentShape;
    use lumino_note_core::automation::{AutomationEdit, AutomationTarget};
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
    let screen = anchor_screen(&editor, &state.bend_path.anchors[0]);
    press(&canvas, &mut state, screen);
    assert!(
        matches!(
            state.bend_path.interaction,
            crate::velocity::widget::bend_path::BendInteraction::DraggingAnchor { .. }
        ),
        "重合锚点可被选中拖动: {:?}",
        state.bend_path.interaction
    );
    let action = move_cursor(&canvas, &mut state, Point::new(screen.x, screen.y - 100.0));
    assert!(action.is_some(), "拖动分离应发 Move 消息");
    assert_ne!(
        state.bend_path.anchors[0].pos.1, state.bend_path.anchors[1].pos.1,
        "拖动后两锚点分离（跳变形成）"
    );
    assert!(
        state.bend_path.anchors[0].pos.1 > state.bend_path.anchors[1].pos.1,
        "向上拖动 = 上行突变（value 增大）"
    );
    assert_eq!(
        state.bend_path.anchors[0].pos.0, state.bend_path.anchors[1].pos.0,
        "同 tick 保持（直角跳变）"
    );
}

#[test]
fn test_bend_downward_jump_keeps_x_position() {
    // 预期行为：B 正下方创建 C → 连线从 B 直线向下（X 不动）连到 C，
    // 再向右延伸——而不是绕过 B
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // A(tick 0) → B(tick 1920)
    press(&canvas, &mut state, Point::new(300.0, 100.0));
    press(&canvas, &mut state, Point::new(500.0, 100.0));

    // B 正下方点击（x=312 → raw tick 1920 → 吸附 1920，与 B 同 tick）
    press(&canvas, &mut state, Point::new(312.0, 300.0));
    assert_eq!(state.bend_path.anchors.len(), 3);
    assert_eq!(
        state.bend_path.anchors[2].pos.0, 1920.0,
        "C 与 B 同 tick（X 方向位置不动）"
    );
    assert!(
        state.bend_path.anchors[1].pos.1 > state.bend_path.anchors[2].pos.1,
        "B(上) → C(下)：向下曲折"
    );
    // 有序：A(0) → B(1920) → C(1920)，B→C 为同 tick 竖直段
    let ticks: Vec<f32> = state.bend_path.anchors.iter().map(|a| a.pos.0).collect();
    assert!(
        ticks.windows(2).all(|w| w[0] <= w[1]),
        "tick 升序: {ticks:?}"
    );
}

#[test]
fn test_bend_vertical_jump_segment_no_insert() {
    // 回归：竖直跳变段（同 tick 两锚点）点击不得插入锚点——
    // 插入位置 tick 必然越出 [tick, tick] 区间（乱序），应拒绝
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 同 tick 跳变对（默认网格吸附 tick 0）
    press(&canvas, &mut state, Point::new(300.0, 100.0)); // (0, 10922)
    press(&canvas, &mut state, Point::new(250.0, 200.0)); // (0, 5461)
    assert_eq!(state.bend_path.anchors.len(), 2);

    // 点击竖直跳变线（x 略偏，命中 Segment；raw tick 越界）
    let v = view_params(&editor);
    let _x = v.tick_to_x(0) + 5.0; // 距锚点 5px（Segment 阈值 8px 内，Anchor 阈值 10px 内?）
    // Anchor 命中半径 10px：5px 会命中锚点，改用 9px
    let x = v.tick_to_x(0) + 9.0;
    let y_mid = (v.value_to_y(10922.0, 16383.0) + v.value_to_y(5461.0, 16383.0)) * 0.5;
    let action = press(&canvas, &mut state, Point::new(x, y_mid));
    assert!(
        action.is_none() || state.bend_path.anchors.len() == 2,
        "竖直跳变段不得插入乱序锚点"
    );
    // 无论命中 Segment（拒绝）还是 Anchor（选中），锚点数不得增加
    assert_eq!(state.bend_path.anchors.len(), 2);
    // 锚点顺序保持 tick 有序
    let ticks: Vec<f32> = state.bend_path.anchors.iter().map(|a| a.pos.0).collect();
    assert!(
        ticks.windows(2).all(|w| w[0] <= w[1]),
        "锚点 tick 必须有序: {ticks:?}"
    );
}

#[test]
fn test_bend_same_tick_different_value_allowed() {
    // 同 tick 不同 value 的锚点合法（竖直跳变段）——网格检查只拦截
    // 完全重合（tick+value 均相同），不误伤合法锚点
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    press(&canvas, &mut state, Point::new(300.0, 100.0));
    assert_eq!(state.bend_path.anchors.len(), 1);
    assert_eq!(state.bend_path.anchors[0].pos.0, 0.0);

    // 同一 tick（吸附 0）不同 value（y=200 → 5461 ≠ 10922）：允许创建
    press(&canvas, &mut state, Point::new(250.0, 200.0));
    assert_eq!(
        state.bend_path.anchors.len(),
        2,
        "同 tick 不同 value 的锚点应允许创建（竖直跳变）"
    );
    assert_eq!(state.bend_path.anchors[1].pos.0, 0.0);
    assert_ne!(
        state.bend_path.anchors[0].pos.1,
        state.bend_path.anchors[1].pos.1
    );
}

#[test]
fn test_bend_max_two_anchors_per_tick() {
    // 同一 tick 最多 2 个锚点（直角弯音跳变对）：
    // 第 1 个锚点（value 10922）→ 第 2 个（value 5461）→ 第 3 个被拒绝
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 同 tick 0（默认网格 1920）不同 value
    press(&canvas, &mut state, Point::new(300.0, 100.0)); // tick 0, value 10922
    press(&canvas, &mut state, Point::new(250.0, 200.0)); // tick 0, value 5461
    assert_eq!(
        state.bend_path.anchors.len(),
        2,
        "同 tick 两个锚点（跳变对）"
    );

    // 第三个同 tick 锚点（不同 value 250.0, 300.0 → value 0）→ 拒绝
    let action = press(&canvas, &mut state, Point::new(200.0, 300.0));
    assert_eq!(
        state.bend_path.anchors.len(),
        2,
        "同 tick 超过 2 个锚点必须被拒绝"
    );
    assert!(action.is_none(), "拒绝创建不应发消息");

    // 但同 tick 两个锚点仍然可以被选中（不影响既有锚点编辑）
    let anchor0 = anchor_screen(&editor, &state.bend_path.anchors[0]);
    press(&canvas, &mut state, anchor0);
    assert_eq!(
        state.bend_path.interaction,
        crate::velocity::widget::bend_path::BendInteraction::DraggingAnchor { idx: 0 },
        "已有锚点仍可选中/拖动"
    );
}
