//! 工具栏交互：走带 / 循环 / 撤销重做 / 右侧开关 / 工具区。
//!
//! 几何全部由 `layout` 常量正推（与绘制同源），点击坐标不靠扫描。
//! 仪表盘 PPQ 编辑的用例在 `toolbar_ppq_edit_bands.rs`（文件行数纪律）。

use egui::{Event, PointerButton, Pos2, RawInput};
use lumino_core::Tool;
use lumino_ui_egui::chrome::ToolbarModel;
use lumino_ui_egui::chrome::toolbar_popup::{ToolbarAction, ToolbarCommand};
use lumino_ui_egui::layout;

mod common;

use common::toolbar_harness::{
    click, click_with_modifiers, group_x, harness, mid_y, model, playback_centers,
};

fn undo_centers() -> [f32; 3] {
    let left = group_x()[3];
    let total = layout::TOOL_BUTTON_W * 3.0;
    let start = left + (layout::UNDO_REDO_GROUP_W - total) / 2.0;
    let w = layout::TOOL_BUTTON_W;
    [start + w / 2.0, start + w * 1.5, start + w * 2.5]
}

#[test]
fn transport_buttons_emit_rewind_play_pause_forward() {
    let [back, play, fwd] = playback_centers();
    let y = mid_y();
    for (pos, expected) in [
        (Pos2::new(back, y), "SkipBackward"),
        (Pos2::new(play, y), "Play"),
        (Pos2::new(fwd, y), "SkipForward"),
    ] {
        let (mut h, actions) = harness(model());
        click(&mut h, pos);
        let got = format!("{:?}", actions.borrow().last());
        assert!(got.contains(expected), "点 {expected} 得到 {got}");
    }

    // 播放中：中间按钮换成暂停，动作改为 Pause。
    let (mut h, actions) = harness(ToolbarModel {
        playing: true,
        ..model()
    });
    click(&mut h, Pos2::new(play, y));
    assert!(matches!(
        actions.borrow().last(),
        Some(ToolbarAction::Pause)
    ));
}

#[test]
fn ctrl_click_on_transport_jumps_to_song_edges() {
    let [back, _, fwd] = playback_centers();
    let y = mid_y();
    let ctrl = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    for (pos, expected) in [
        (Pos2::new(back, y), ToolbarAction::SkipBackwardToStart),
        (Pos2::new(fwd, y), ToolbarAction::SkipForwardToEnd),
    ] {
        let (mut h, actions) = harness(model());
        click_with_modifiers(&mut h, pos, ctrl);
        assert_eq!(actions.borrow().last(), Some(&expected));
    }
}

#[test]
fn loop_undo_redo_history_and_record_emit_actions() {
    let y = mid_y();
    let groups = group_x();

    let (mut h, actions) = harness(model());
    click(&mut h, Pos2::new(groups[2] + layout::LOOP_GROUP_W / 2.0, y));
    assert!(matches!(
        actions.borrow().last(),
        Some(ToolbarAction::ToggleLoop)
    ));

    for (index, expected) in [
        (0usize, ToolbarAction::Undo),
        (1, ToolbarAction::Redo),
        (2, ToolbarAction::OpenHistory),
    ] {
        let (mut h, actions) = harness(model());
        click(&mut h, Pos2::new(undo_centers()[index], y));
        assert_eq!(actions.borrow().last(), Some(&expected));
    }

    let (mut h, actions) = harness(model());
    click(
        &mut h,
        Pos2::new(groups[0] + layout::RECORD_GROUP_W / 2.0, y),
    );
    assert!(matches!(
        actions.borrow().last(),
        Some(ToolbarAction::RecordToggled)
    ));
}

#[test]
fn bottom_strip_drives_height_resize() {
    // 底部 6px：按下 → ResizeStarted；拖拽 → ResizeDragged；松开 → ResizeEnded。
    let (mut h, actions) = harness(model());
    let pos = Pos2::new(700.0, layout::TOOLBAR_H - 3.0);
    let mut input = RawInput::default();
    input.events.push(Event::PointerMoved(pos));
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    *h.input_mut() = input;
    h.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|a| matches!(a, ToolbarAction::ResizeStarted)),
        "按下底边应开始调整高度：{:?}",
        actions.borrow()
    );

    let moved = Pos2::new(700.0, layout::TOOLBAR_H + 10.0);
    let mut input = RawInput::default();
    input.events.push(Event::PointerMoved(moved));
    *h.input_mut() = input;
    h.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|a| matches!(a, ToolbarAction::ResizeDragged(_))),
        "拖拽应回传光标 y：{:?}",
        actions.borrow()
    );

    let mut input = RawInput::default();
    input.events.push(Event::PointerButton {
        pos: moved,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    *h.input_mut() = input;
    h.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|a| matches!(a, ToolbarAction::ResizeEnded)),
        "松开应结束调整：{:?}",
        actions.borrow()
    );
}

#[test]
fn auto_scroll_and_collaboration_emit_actions() {
    let y = mid_y();
    // 自动滚动固定在右侧（与「虚拟 More」为基准）。
    let more_left = 1400.0 - layout::TOOLBAR_PAD_X - layout::MORE_BUTTON_W;
    let scroll = Pos2::new(
        more_left - 36.0 - layout::RIGHT_TOOL_W + layout::RIGHT_TOOL_W / 2.0,
        y,
    );
    let (mut h, actions) = harness(model());
    click(&mut h, scroll);
    assert!(matches!(
        actions.borrow().last(),
        Some(ToolbarAction::AutoScrollModeChanged)
    ));

    // 无溢出时协作按钮补位到最右。
    let collab = Pos2::new(
        1400.0 - layout::TOOLBAR_PAD_X - 4.0 - layout::RIGHT_TOOL_W / 2.0,
        y,
    );
    let (mut h, actions) = harness(ToolbarModel {
        more_visible: false,
        collaboration_visible: true,
        ..model()
    });
    click(&mut h, collab);
    assert!(matches!(
        actions.borrow().last(),
        Some(ToolbarAction::OpenCollaborationDialog)
    ));
}

#[test]
fn tool_area_emits_selection_and_commands() {
    let y = mid_y();
    let tools_left = group_x()[5];
    // 横向扫过工具区：收集每次命中的动作，验证顺序与**命中区宽度**。
    //
    // 这里锁的是一个真实回归：egui `Button` 会把 `spacing.interact_size`（40×18）
    // 与 frame 内边距算进最小尺寸，导致选中态按钮占 42px、相邻按钮互相抢点击。
    // 自绘后各选择器命中区宽度必须彼此接近（差值超过 6px 即意味着又回到老问题）。
    let mut hits: Vec<(f32, ToolbarAction)> = Vec::new();
    let mut x = tools_left + 1.0;
    while x < tools_left + 360.0 {
        let (mut h, actions) = harness(model());
        click(&mut h, Pos2::new(x, y));
        if let Some(action) = actions.borrow().last() {
            hits.push((x, action.clone()));
        }
        x += 2.0;
    }
    let mut sequence: Vec<ToolbarAction> = Vec::new();
    for (_, action) in &hits {
        if sequence.last() != Some(action) {
            sequence.push(action.clone());
        }
    }
    let expected = [
        ToolbarAction::ToolSelected(Tool::Pointer),
        ToolbarAction::ToolSelected(Tool::PointerYSelect),
        ToolbarAction::ToolSelected(Tool::Pencil),
        ToolbarAction::ToolSelected(Tool::Eraser),
        ToolbarAction::ToggleToolPanel,
        ToolbarAction::ToolCommand(ToolbarCommand::Quantize),
        ToolbarAction::ToolCommand(ToolbarCommand::SpeedChange),
        ToolbarAction::ToolCommand(ToolbarCommand::FlipVertical),
        ToolbarAction::ToolCommand(ToolbarCommand::FlipHorizontalCenter),
    ];
    assert!(
        sequence.starts_with(&expected),
        "工具区动作顺序不符：{sequence:?}"
    );

    // 四个工具选择器的命中区宽度必须一致（选中态不得更宽）。
    let spans: Vec<f32> = expected[..4]
        .iter()
        .map(|action| {
            let xs: Vec<f32> = hits
                .iter()
                .filter(|(_, a)| a == action)
                .map(|(x, _)| *x)
                .collect();
            xs.last().copied().unwrap_or(0.0) - xs.first().copied().unwrap_or(0.0) + 2.0
        })
        .collect();
    let (min, max) = spans
        .iter()
        .fold((f32::MAX, 0.0_f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    assert!(
        max - min <= 6.0,
        "工具选择器命中区宽度不一致（{spans:?}），相邻按钮可能互抢点击"
    );
    assert!(
        min >= layout::TOOL_SELECTOR_W - 4.0,
        "命中区过窄（{spans:?}），按钮被裁切"
    );
}
