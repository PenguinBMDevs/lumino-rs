//! 力度面板双向滚轮与 Tempo BPM 上限测试

use super::*;
use crate::editor::velocity::EditMode;
use crate::message::VelocityAction;

/// 双向滚轮（对角线）：水平分量滚动时间轴，垂直分量滚动自动化曲线，同时生效
#[test]
fn test_velocity_wheel_scrolled_bidirectional() {
    let mut root = create_root();
    // 水平滚动需要横向内容空间（与网格测试一致）
    root.editor.editor_state.canvas.size_x = 1000.0;
    // 垂直滚动需要 zoom > 1 才有滚动余量（默认 zoom=1.0 时可见范围=满量程，会被 clamp 到 0）
    root.editor.velocity_panel.value_zoom = 2.0;
    root.editor.velocity_panel.edit_mode = EditMode::Cc(1);

    let before_x = root.editor.editor_state.view.smooth_scroll.target_x;
    VelocityHandler::new().handle(
        &mut root,
        Message::Velocity(VelocityAction::WheelScrolled {
            delta_x: -100.0,
            delta_y: -1.0, // 上滑 → 自动化曲线 value_scroll 增大
        }),
    );

    // 水平：左滑 → scroll_x 增大（内容跟随手指）
    assert!(
        root.editor.editor_state.view.smooth_scroll.target_x > before_x,
        "水平分量应滚动时间轴，target_x={}",
        root.editor.editor_state.view.smooth_scroll.target_x
    );
    // 垂直：自动化曲线滚动（CC 模式生效）
    assert!(
        root.editor.velocity_panel.value_scroll > 0.0,
        "垂直分量应滚动自动化曲线，value_scroll={}",
        root.editor.velocity_panel.value_scroll
    );
}

/// 双向滚轮：Velocity 模式垂直分量不生效（保持无操作语义），水平分量仍生效
#[test]
fn test_velocity_wheel_scrolled_vertical_ignored_in_velocity_mode() {
    let mut root = create_root();
    root.editor.editor_state.canvas.size_x = 1000.0;
    root.editor.velocity_panel.edit_mode = EditMode::Velocity;

    let before_x = root.editor.editor_state.view.smooth_scroll.target_x;
    VelocityHandler::new().handle(
        &mut root,
        Message::Velocity(VelocityAction::WheelScrolled {
            delta_x: -100.0,
            delta_y: 1.0,
        }),
    );

    assert_eq!(
        root.editor.velocity_panel.value_scroll, 0.0,
        "Velocity 模式垂直分量应被忽略"
    );
    assert!(
        root.editor.editor_state.view.smooth_scroll.target_x > before_x,
        "水平分量仍应滚动时间轴"
    );
}

// ── Tempo 面板 BPM 上限（BUG 回归：硬编码 10000 截断） ──────────────────
/// BUG 复现：用户把 Tempo 面板绘制上限（tempo_max_bpm，设置里可调至 65536）
/// 调高后，拖拽速度点仍被旧硬编码 `clamp(20.0, 10000.0)` 截断，
/// 曲线永远无法到达面板顶部，表现为"最大绘制值只能到 10000"。
///
/// 修复前：面板上限 20000 时，拖到 30000 只能得到 10000。
#[test]
fn test_tempo_drag_move_uses_panel_max_bpm() {
    let mut root = create_root();
    root.editor.velocity_panel.tempo_max_bpm = 20000.0;

    VelocityHandler::handle_action(&mut root, VelocityAction::TempoDragMove(0, 30000.0));

    let bpm = root.editor.editor_state.data.tempo_points[0].bpm;
    assert_eq!(
        bpm, 20000.0,
        "拖拽值应截断到面板绘制上限，而非旧硬编码 10000"
    );
}

/// 同类路径：新建速度点同样按面板绘制上限截断
#[test]
fn test_tempo_add_uses_panel_max_bpm() {
    let mut root = create_root();
    root.editor.velocity_panel.tempo_max_bpm = 20000.0;

    VelocityHandler::handle_action(&mut root, VelocityAction::TempoAdd(480.0, 50000.0));

    let bpm = root
        .editor
        .editor_state
        .data
        .tempo_points
        .iter()
        .find(|p| (p.tick - 480.0).abs() < f32::EPSILON)
        .map(|p| p.bpm)
        .expect("TempoAdd 后应存在 tick=480 的速度点");
    assert_eq!(bpm, 20000.0, "新建点应截断到面板绘制上限");
}

/// 默认上限 512 时行为保持不变：超出上限的值截断到 512
#[test]
fn test_tempo_clamp_uses_default_max_bpm() {
    let mut root = create_root();

    VelocityHandler::handle_action(&mut root, VelocityAction::TempoDragMove(0, 600.0));

    assert_eq!(
        root.editor.editor_state.data.tempo_points[0].bpm, 512.0,
        "默认上限 512 下 600 应截断到 512"
    );
}

/// 下限保持 TEMPO_BPM_MIN（20）：低于下限的值截断到 20
#[test]
fn test_tempo_clamp_min_bpm() {
    let mut root = create_root();

    VelocityHandler::handle_action(&mut root, VelocityAction::TempoDragMove(0, 5.0));

    assert_eq!(
        root.editor.editor_state.data.tempo_points[0].bpm, 20.0,
        "低于下限的值应截断到 20"
    );
}
