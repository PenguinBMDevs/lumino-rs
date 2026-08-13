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
fn test_bend_same_height_click_creates_jump_pair() {
    // 回归：同一 Y 高度（吸附后与已有锚点完全重合）点击必须创建第二个
    // 锚点（跳变对的初始状态，与第一个重叠显示）——随后拖动分离形成
    // 上行的突变。旧实现直接放弃创建（anchor_at_pos 拒绝）。
    // 默认网格 = 四分音符 1920 ticks、zoom_x=0.1、keyboard_width=120：
    // 点击 (300,100) → tick 1800 → 吸附 0，value 10922；
    // 点击 (250,100) → tick 1300 → 吸附仍 0，value 相同。
    let mut editor = crate::Editor::new();
    editor.set_tool(Tool::Curve);
    let canvas = bend_canvas(&editor);
    let mut state = widget::VelocityCanvasState::default();

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

    // 拖动锚点上行（y 减小 = value 增大）→ 分离形成上行突变。
    // 重合状态下点击命中第一个锚点（hit test 顺序），拖动它分离即可。
    use lumino_note_core::SegmentShape;
    use lumino_note_core::automation::{AutomationEdit, AutomationTarget};
    drop(canvas);
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
            BendInteraction::DraggingAnchor { .. }
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
        BendInteraction::DraggingAnchor { idx: 2 }
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
    let (ov1, nv1) = extract_move(move1.unwrap());
    let (ov2, nv2) = extract_move(move2.unwrap());
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
    let x = v.tick_to_x(0) + 5.0; // 距锚点 5px（Segment 阈值 8px 内，Anchor 阈值 10px 内?）
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
        BendInteraction::DraggingAnchor { idx: 0 },
        "已有锚点仍可选中/拖动"
    );
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
