//! 滚动相关测试

use crate::Editor;
use lumino_editor_state::editor_state::viewport::Viewport;
use lumino_ui_core::message::EditorAction;

/// 测试滚动边界
#[test]
fn test_scroll_boundaries() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.editor_state.view.total_ticks = 1000;

    // 设置一个超出范围的 scroll_x
    editor.set_scroll_x(10000.0, 120.0, 800.0, 1.0);

    // 应该被限制在有效范围内
    assert!(editor.scroll_x() <= editor.editor_state.max_scroll.0);
    assert!(editor.scroll_x() >= 0.0);
}

/// 复现：标尺区普通滚轮水平平移（启动平滑滚动动画）后立即 Ctrl+滚轮缩放，
/// 缩放必须终止遗留的平滑滚动动画——否则动画会把 scroll_x 拉回旧目标，
/// 表现为"Ctrl 缩放的同时卷帘还在左右滚动"（bug）。
#[test]
fn test_zoom_x_stops_active_scroll_animation() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.editor_state.view.total_ticks = 100000;
    {
        let state = &mut editor.editor_state;
        let total_ticks = state.view.total_ticks;
        Viewport::new(&mut state.view, &mut state.max_scroll).update_max_scroll(total_ticks);
    }

    // 1) 标尺普通滚轮（无 Ctrl）：水平平移 → 启动平滑滚动动画
    editor.handle_action(EditorAction::Scrolled {
        delta_x: -120.0,
        delta_y: 0.0,
    });
    assert!(
        editor.editor_state.view.smooth_scroll.active,
        "普通滚动应启动平滑动画"
    );

    // 2) 按住 Ctrl 滚轮缩放 X 轴（锚点固定缩放）
    let zoom_before = editor.editor_state.view.zoom_x;
    editor.set_zoom_x(zoom_before * 1.1, 0.5);
    let scroll_after_zoom = editor.editor_state.view.scroll_x;

    // 3) 推进一帧动画（handle_animation_tick 的等价物）
    let v = &mut editor.editor_state.view;
    let (new_x, _, _) = v.smooth_scroll.update(v.scroll_x, v.scroll_y);
    v.scroll_x = new_x;

    // 缩放后 scroll_x 必须保持缩放锚点补偿结果，不被遗留动画拉回
    assert!(
        (v.scroll_x - scroll_after_zoom).abs() < 0.5,
        "缩放后遗留动画不应继续滚动 scroll_x: 缩放后={scroll_after_zoom}, 动画推进后={}",
        v.scroll_x
    );
}

/// 同 X 轴：Y 向缩放（键盘区 Ctrl+滚轮）同样必须终止遗留的平滑滚动动画
#[test]
fn test_zoom_y_stops_active_scroll_animation() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.editor_state.view.total_ticks = 100000;
    {
        let state = &mut editor.editor_state;
        let total_ticks = state.view.total_ticks;
        Viewport::new(&mut state.view, &mut state.max_scroll).update_max_scroll(total_ticks);
    }

    // 1) 主区域普通滚轮：垂直滚动 → 启动平滑滚动动画
    editor.handle_action(EditorAction::Scrolled {
        delta_x: 0.0,
        delta_y: -120.0,
    });
    assert!(
        editor.editor_state.view.smooth_scroll.active,
        "普通滚动应启动平滑动画"
    );

    // 2) 键盘区 Ctrl+滚轮缩放 Y 轴（锚点固定缩放）
    let zoom_before = editor.editor_state.view.zoom_y;
    editor.set_zoom_y(zoom_before * 1.1, 0.5);
    let scroll_after_zoom = editor.editor_state.view.scroll_y;

    // 3) 推进一帧动画
    let v = &mut editor.editor_state.view;
    let (_, new_y, _) = v.smooth_scroll.update(v.scroll_x, v.scroll_y);
    v.scroll_y = new_y;

    // 缩放后 scroll_y 必须保持缩放锚点补偿结果，不被遗留动画拉回
    assert!(
        (v.scroll_y - scroll_after_zoom).abs() < 0.5,
        "缩放后遗留动画不应继续滚动 scroll_y: 缩放后={scroll_after_zoom}, 动画推进后={}",
        v.scroll_y
    );
}
