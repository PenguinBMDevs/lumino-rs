//! 钢琴卷帘网格滚轮交互单元测试
//!
//! 从 grid/program.rs 主文件拆出，因为主文件超过 400 行。

use super::PianoRollGrid;
use crate::Editor;
use iced_core::Point;
use iced_core::mouse::ScrollDelta;
use lumino_ui_core::Message;
use lumino_ui_core::constants::editor::SCROLL_LINES_SCALE;

#[test]
fn test_wheel_delta_lines_scale() {
    let (dx, dy) = PianoRollGrid::wheel_delta(&ScrollDelta::Lines { x: 2.0, y: -1.0 });
    assert_eq!(dx, 2.0 * SCROLL_LINES_SCALE);
    assert_eq!(dy, -SCROLL_LINES_SCALE);
}

#[test]
fn test_wheel_delta_pixels_unchanged() {
    let (dx, dy) = PianoRollGrid::wheel_delta(&ScrollDelta::Pixels { x: 10.0, y: -25.0 });
    assert_eq!(dx, 10.0);
    assert_eq!(dy, -25.0);
}

/// 缩放因子/锚点比例的公共逻辑已迁移至 crate::zoom（见 zoom.rs 测试），
/// 此处仅保留钢琴卷帘网格自身的滚轮行为测试。
#[test]
fn test_keyboard_wheel_without_ctrl_is_noop() {
    // 键盘区域未按 Ctrl 时滚轮不产生任何动作（保持原有行为）
    let editor = Editor::new();
    let grid = PianoRollGrid::new(&editor);
    let action = grid.handle_keyboard_wheel_scroll(
        &ScrollDelta::Lines { x: 0.0, y: 1.0 },
        false,
        Point::new(30.0, 300.0),
    );
    assert!(action.is_none());
}

/// 互斥隔离：标尺区 Ctrl+滚轮只产生缩放，绝不产生平移（Scrolled）
#[test]
fn test_ruler_wheel_ctrl_zooms_only() {
    let editor = Editor::new();
    let grid = PianoRollGrid::new(&editor);
    let action = grid
        .handle_ruler_wheel_scroll(
            &ScrollDelta::Lines { x: 0.0, y: 1.0 },
            true,
            Point::new(430.0, 20.0),
        )
        .expect("Ctrl+滚轮应产生动作");
    // 展开 Action 检查是否只有 ZoomXChanged（缩放与平移互斥，二者不会同时发出）
    let (message, _, _) = action.into_inner();
    match message {
        Some(Message::ZoomXChanged { zoom, fixed_ratio }) => {
            assert!(zoom > 0.0);
            assert!((0.0..=1.0).contains(&fixed_ratio));
        }
        other => panic!("Ctrl+滚轮标尺区应只发 ZoomXChanged，实际为: {other:?}"),
    }
}

/// 互斥隔离：标尺区无 Ctrl 滚轮只产生水平平移，绝不产生缩放
#[test]
fn test_ruler_wheel_without_ctrl_pans_only() {
    let editor = Editor::new();
    let grid = PianoRollGrid::new(&editor);
    let action = grid
        .handle_ruler_wheel_scroll(
            &ScrollDelta::Lines { x: 0.0, y: 1.0 },
            false,
            Point::new(430.0, 20.0),
        )
        .expect("普通滚轮应产生动作");
    let (message, _, _) = action.into_inner();
    match message {
        Some(Message::EditorAction(lumino_ui_core::message::EditorAction::Scrolled {
            delta_x,
            delta_y,
        })) => {
            // 向上滚 → 发送 delta_x < 0（handle_scrolled 取反后 scroll_x 增大、视图右移），且无垂直分量
            assert!(delta_x < 0.0);
            assert_eq!(delta_y, 0.0);
        }
        other => panic!("无 Ctrl 标尺区应只发 Scrolled，实际为: {other:?}"),
    }
}

/// 触控板水平滑动方向（回归测试：左滑/右滑应内容跟随手指）：
/// 1) 网格区收到触控板像素增量 → 原样透传 delta_x（左滑为负）；
/// 2) Editor 消费后 scroll_x 符号与 delta_x 相反（左滑 → scroll_x 增大）。
#[test]
fn test_grid_wheel_horizontal_swipe_follows_finger() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 1000.0;
    editor.editor_state.canvas.size_y = 500.0;
    let grid = PianoRollGrid::new(&editor);

    // 触控板左滑（像素增量 x < 0）
    let action = grid
        .handle_wheel_scroll(&ScrollDelta::Pixels { x: -100.0, y: 0.0 }, false)
        .expect("触控板水平滑动应产生动作");
    let (message, _, _) = action.into_inner();
    let (delta_x, delta_y) = match message {
        Some(Message::EditorAction(lumino_ui_core::message::EditorAction::Scrolled {
            delta_x,
            delta_y,
        })) => (delta_x, delta_y),
        other => panic!("网格区滚轮应发 Scrolled，实际为: {other:?}"),
    };
    assert!(delta_x < 0.0, "左滑应产生负 delta_x，实际={delta_x}");
    assert_eq!(delta_y, 0.0);

    // Editor 消费后：scroll_x 增大（内容跟随手指左移，显示更后音符）
    editor.handle_action(lumino_ui_core::message::EditorAction::Scrolled { delta_x, delta_y });
    assert!(
        editor.editor_state.view.smooth_scroll.target_x > 0.0,
        "左滑后 scroll_x 应增大，实际 target_x={}",
        editor.editor_state.view.smooth_scroll.target_x
    );
}

/// 互斥隔离：Ctrl 状态下普通平移分支绝不生效——
/// Ctrl+滚轮与普通滚轮不可叠加，同一事件至多产生一种动作
#[test]
fn test_ruler_wheel_actions_are_exclusive() {
    let editor = Editor::new();
    let grid = PianoRollGrid::new(&editor);
    // 同一个滚轮事件，在 Ctrl 按下/松开两种状态下产生且只产生一种动作
    let ctrl_action = grid.handle_ruler_wheel_scroll(
        &ScrollDelta::Lines { x: 0.0, y: -1.0 },
        true,
        Point::new(430.0, 20.0),
    );
    let plain_action = grid.handle_ruler_wheel_scroll(
        &ScrollDelta::Lines { x: 0.0, y: -1.0 },
        false,
        Point::new(430.0, 20.0),
    );
    let ctrl_msg = ctrl_action.expect("Ctrl+滚轮应产生动作").into_inner().0;
    let plain_msg = plain_action.expect("普通滚轮应产生动作").into_inner().0;
    assert!(matches!(ctrl_msg, Some(Message::ZoomXChanged { .. })));
    assert!(matches!(
        plain_msg,
        Some(Message::EditorAction(
            lumino_ui_core::message::EditorAction::Scrolled { .. }
        ))
    ));
}

/// 斜向滚动（触控板双指对角线滑动）：单条事件携带双轴非零分量，
/// 必须同时驱动 X 轴与 Y 轴滚动——这是「斜向滚动」的核心验收点。
#[test]
fn test_grid_wheel_diagonal_scrolls_both_axes() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 1000.0;
    // 制造足够内容使 max_scroll 双轴均 > 0（否则会被 clamp 到 0 导致滚动失效）
    editor.editor_state.view.total_ticks = 100000;
    {
        let state = &mut editor.editor_state;
        let total_ticks = state.view.total_ticks;
        lumino_editor_state::editor_state::viewport::Viewport::new(
            &mut state.view,
            &mut state.max_scroll,
        )
        .update_max_scroll(total_ticks);
    }
    let grid = PianoRollGrid::new(&editor);

    // 触控板左滑+上滑（像素增量 x<0, y<0）
    let action = grid
        .handle_wheel_scroll(
            &ScrollDelta::Pixels {
                x: -100.0,
                y: -50.0,
            },
            false,
        )
        .expect("斜向滚动应产生动作");
    let (message, _, _) = action.into_inner();
    let (delta_x, delta_y) = match message {
        Some(Message::EditorAction(lumino_ui_core::message::EditorAction::Scrolled {
            delta_x,
            delta_y,
        })) => (delta_x, delta_y),
        other => panic!("网格区滚轮应发 Scrolled，实际为: {other:?}"),
    };
    // 双轴分量都应非零
    assert!(delta_x < 0.0, "左滑应产生负 delta_x，实际={delta_x}");
    assert!(delta_y < 0.0, "上滑应产生负 delta_y，实际={delta_y}");

    // Editor 消费后：双轴目标位置都应变化（斜向滚动生效）
    editor.handle_action(lumino_ui_core::message::EditorAction::Scrolled { delta_x, delta_y });
    assert!(
        editor.editor_state.view.smooth_scroll.target_x > 0.0,
        "斜向滚动后 scroll_x 应增大，实际 target_x={}",
        editor.editor_state.view.smooth_scroll.target_x
    );
    assert!(
        editor.editor_state.view.smooth_scroll.target_y > 0.0,
        "斜向滚动后 scroll_y 应增大，实际 target_y={}",
        editor.editor_state.view.smooth_scroll.target_y
    );
}
