//! 弯音贝塞尔路径交互集成测试
//!
//! 覆盖用户反馈的交互行为：
//! - 连续点击空白：每次追加一个锚点（可无限放置，形成线段）；
//! - 按下+松开不应产生重合锚点；
//! - 点击锚点选中（高亮状态）；
//! - 双击中间锚点删除。

use iced_core::{Point, Size};
use iced_widget::canvas;

use crate::velocity::EditMode;
use crate::velocity::widget::bend_path::BendInteraction;
use lumino_core::Tool;
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::widget;

/// 构造 Bend 模式 Curve 工具的 Canvas
fn bend_canvas<'a>(editor: &'a crate::Editor) -> widget::VelocityCanvas<'a> {
    widget::VelocityCanvas {
        editor,
        edit_mode: EditMode::Bend,
        selected_cc: 1,
    }
}

fn bounds() -> Size {
    Size::new(800.0, 300.0)
}

/// 构造与面板一致的 AutomationViewParams（默认视图：zoom_x=0.1, keyboard_width=120）
/// 约定与 `automation_view_params` 一致：panel_height = canvas 高度、toolbar_height = 0。
fn view_params(editor: &crate::Editor) -> lumino_gfx::automation::AutomationViewParams {
    let view = &editor.editor_state.view;
    lumino_gfx::automation::AutomationViewParams {
        panel_height: bounds().height,
        pixels_per_tick: view.zoom_x,
        scroll_x: view.scroll_x,
        keyboard_width: view.keyboard_width,
        value_zoom: 1.0,
        value_scroll: 0.0,
        panel_offset_x: 0.0,
        panel_offset_y: 0.0,
        toolbar_height: 0.0,
        line_thickness: 2.0,
    }
}

/// 模拟按下（返回 Action 消息）
fn press<'a>(
    canvas: &widget::VelocityCanvas<'a>,
    state: &mut widget::VelocityCanvasState,
    pos: Point,
) -> Option<canvas::Action<Message>> {
    canvas.handle_button_pressed(state, pos, &iced_core::mouse::Cursor::Unavailable, bounds())
}

/// 模拟松开
fn release<'a>(
    canvas: &widget::VelocityCanvas<'a>,
    state: &mut widget::VelocityCanvasState,
) -> Option<canvas::Action<Message>> {
    canvas.handle_button_released(state, bounds())
}

/// 模拟鼠标移动（拖动）
fn move_cursor<'a>(
    canvas: &widget::VelocityCanvas<'a>,
    state: &mut widget::VelocityCanvasState,
    pos: Point,
) -> Option<canvas::Action<Message>> {
    canvas.handle_cursor_moved(
        state,
        pos,
        &iced_core::mouse::Cursor::Available(pos),
        bounds(),
    )
}

/// 从 Action 中提取 VelocityAction（消费 action）
fn velocity_action(action: canvas::Action<Message>) -> Option<VelocityAction> {
    let (msg, _, _) = action.into_inner();
    match msg {
        Some(Message::Velocity(action)) => Some(action),
        _ => None,
    }
}

/// 锚点逻辑坐标 → 面板局部屏幕坐标
fn anchor_screen(
    editor: &crate::Editor,
    anchor: &crate::velocity::widget::bend_path::BendAnchor,
) -> Point {
    let v = view_params(editor);
    Point::new(
        v.tick_to_x(anchor.pos.0.round() as u32),
        v.value_to_y(anchor.pos.1, 16383.0),
    )
}

#[test]
fn test_bend_click_appends_anchor() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 第一次点击空白：追加锚点 1，选中，发 Add 消息
    let action = press(&canvas, &mut state, Point::new(300.0, 100.0));
    assert!(action.is_some());
    assert_eq!(state.bend_path.anchors.len(), 1);
    assert_eq!(state.bend_path.selected, Some(0));
    assert!(matches!(
        velocity_action(action.unwrap()),
        Some(VelocityAction::AutomationEdit(_))
    ));

    // 第二次点击空白：追加锚点 2（形成线段）
    let action = press(&canvas, &mut state, Point::new(500.0, 200.0));
    assert!(action.is_some());
    assert_eq!(state.bend_path.anchors.len(), 2, "每次点击应追加一个锚点");
    assert_eq!(state.bend_path.selected, Some(1));

    // 第三次：继续追加（无限放置）
    let action = press(&canvas, &mut state, Point::new(700.0, 50.0));
    assert!(action.is_some());
    assert_eq!(state.bend_path.anchors.len(), 3);
    // 锚点按点击顺序排列（tick 递增）
    let ticks: Vec<u32> = state
        .bend_path
        .anchors
        .iter()
        .map(|a| a.pos.0 as u32)
        .collect();
    assert!(
        ticks.windows(2).all(|w| w[0] < w[1]),
        "锚点 tick 应递增: {ticks:?}"
    );
}

#[test]
fn test_bend_press_release_no_duplicate_anchor() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 按下 + 松开：不应产生重合锚点（回归：旧实现松开时创建第二个重合锚点）
    let action = press(&canvas, &mut state, Point::new(300.0, 100.0));
    assert!(action.is_some());
    assert_eq!(state.bend_path.anchors.len(), 1);
    let action = release(&canvas, &mut state);
    assert!(action.is_none(), "松开不应产生消息");
    assert_eq!(
        state.bend_path.anchors.len(),
        1,
        "按下+松开后应仍只有 1 个锚点"
    );
    assert_eq!(state.bend_path.interaction, BendInteraction::None);
}

#[test]
fn test_bend_click_anchor_selects() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 先放置两个锚点
    press(&canvas, &mut state, Point::new(300.0, 100.0));
    press(&canvas, &mut state, Point::new(500.0, 200.0));

    // 点击第一个锚点：选中（锚点屏幕位置 = tick*zoom + keyboard_width, value→y）
    let screen = anchor_screen(&editor, &state.bend_path.anchors[0]);
    let action = press(&canvas, &mut state, screen);
    assert!(action.is_some(), "点击锚点应开始拖拽");
    assert_eq!(state.bend_path.selected, Some(0), "点击锚点应选中它");
    assert_eq!(
        state.bend_path.interaction,
        BendInteraction::DraggingAnchor { idx: 0 }
    );
}

#[test]
fn test_bend_double_click_deletes_middle_anchor() {
    use lumino_note_core::SegmentShape;
    use lumino_note_core::automation::{AutomationEdit, AutomationTarget};

    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    // 预建 Bend lane（模拟已放置的锚点），使删除消息能定位 lane
    editor
        .editor_state
        .data
        .apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::PitchBend,
            channel: 0,
            tick: 480,
            value: 8192,
            shape: SegmentShape::Curve { tension: 0 },
        });
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 放置 3 个锚点
    press(&canvas, &mut state, Point::new(300.0, 100.0));
    press(&canvas, &mut state, Point::new(500.0, 200.0));
    press(&canvas, &mut state, Point::new(700.0, 50.0));
    assert_eq!(state.bend_path.anchors.len(), 3);

    // 双击中间锚点（第二次按下触发 detect_double_click）
    let screen = anchor_screen(&editor, &state.bend_path.anchors[1]);
    // 第一次按下记录点击，第二次按下检测双击
    press(&canvas, &mut state, screen);
    let action = press(&canvas, &mut state, screen);
    assert!(action.is_some(), "双击删除应产生 Delete 消息");
    assert_eq!(state.bend_path.anchors.len(), 2, "中间锚点应被删除");
    assert!(
        matches!(
            velocity_action(action.unwrap()),
            Some(VelocityAction::AutomationEdit(_))
        ),
        "删除应发 AutomationEdit::Delete"
    );
}

#[test]
fn test_bend_segment_click_inserts_anchor() {
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    // 两个锚点形成线段
    press(&canvas, &mut state, Point::new(300.0, 100.0));
    press(&canvas, &mut state, Point::new(700.0, 100.0));
    assert_eq!(state.bend_path.anchors.len(), 2);

    // 点击线段中点：插入锚点
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
    assert_eq!(state.bend_path.anchors.len(), 3);
    assert_eq!(state.bend_path.selected, Some(1), "插入的锚点应被选中");
}

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
fn test_bend_no_duplicate_anchor_at_same_grid() {
    // 回归：网格吸附导致重复创建重叠锚点。
    // 默认网格 = 四分音符 1920 ticks、zoom_x=0.1、keyboard_width=120：
    // 点击 (300,100) → tick=(300-120)/0.1=1800 → 吸附 0；
    // 点击 (250,100) → tick=1300 → 吸附仍是 0，且 value 相同 →
    // 新锚点与已有锚点完全重合 → 不得创建，改为选中已有锚点。
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

    press(&canvas, &mut state, Point::new(300.0, 100.0));
    assert_eq!(state.bend_path.anchors.len(), 1);
    assert_eq!(state.bend_path.anchors[0].pos.0, 0.0, "锚点吸附到 tick 0");
    let value = state.bend_path.anchors[0].pos.1;

    // 点击同一网格点（距锚点屏幕 130px，不命中任何元素；吸附后重合）
    let action = press(&canvas, &mut state, Point::new(250.0, 100.0));
    assert_eq!(
        state.bend_path.anchors.len(),
        1,
        "吸附重合位置不得重复创建锚点"
    );
    assert_eq!(state.bend_path.selected, Some(0), "应选中已有锚点");
    assert!(action.is_none(), "不创建 → 不应发消息");
    assert_eq!(state.bend_path.anchors[0].pos.1, value, "已有锚点不被修改");

    // 同一网格点再次点击：仍然不创建
    press(&canvas, &mut state, Point::new(200.0, 100.0));
    assert_eq!(state.bend_path.anchors.len(), 1, "重复点击仍不创建");
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
    let action = action.unwrap();
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
