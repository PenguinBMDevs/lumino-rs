//! 坐标转换与绘图相关测试

use crate::constants::editor::zoom;
use crate::editor::Editor;

/// 测试坐标转换：tick 到 x 坐标
#[test]
fn test_tick_to_x_conversion() {
    let editor = Editor::new();
    let v = &editor.editor_state.view;
    let tick = 100.0;
    let expected_x = tick * v.zoom_x + v.keyboard_width - v.scroll_x;
    assert_eq!(editor.tick_to_x(tick), expected_x);
}

/// 测试坐标转换：x 到 tick
#[test]
fn test_x_to_tick_conversion() {
    let editor = Editor::new();
    let v = &editor.editor_state.view;
    let x = 200.0;
    let expected_tick = (x - v.keyboard_width + v.scroll_x) / v.zoom_x;
    assert_eq!(editor.x_to_tick(x), expected_tick);
}

/// 测试坐标转换：双向转换应该保持一致
#[test]
fn test_tick_x_roundtrip() {
    let editor = Editor::new();
    let original_tick = 480.0;
    let x = editor.tick_to_x(original_tick);
    let recovered_tick = editor.x_to_tick(x);

    // 允许浮点误差
    assert!(
        (original_tick - recovered_tick).abs() < 0.01,
        "Roundtrip failed: original={}, recovered={}",
        original_tick,
        recovered_tick
    );
}

/// 测试 key 到 y 坐标转换
#[test]
fn test_key_to_y_conversion() {
    let editor = Editor::new();
    let v = &editor.editor_state.view;
    let key = 60u16; // 中央 C
    let max_key_index = (v.visible_key_count - 1) as f32;
    let expected_y = (max_key_index - key as f32) * v.zoom_y - v.scroll_y + v.ruler_height;
    assert_eq!(editor.key_to_y(key), expected_y);
}

/// 测试 y 坐标到 key 转换
#[test]
fn test_y_to_key_conversion() {
    let editor = Editor::new();
    let y = 100.0;
    let key = editor.y_to_key(y);

    // 确保 key 在有效范围内
    assert!(key < editor.editor_state.view.visible_key_count);
}

/// 测试 tick 吸附功能
#[test]
fn test_snap_tick() {
    let mut editor = Editor::new();
    // 使用 setter 确保新旧状态同步
    editor.set_snap_precision(120.0); // 1/4 音符

    // 测试在精度区域内向下吸附到区域起始位置
    let tick1 = 130.0;
    let snapped1 = editor.snap_tick(tick1);
    assert_eq!(snapped1, 120.0);

    // 测试在精度区域中间仍然吸附到区域起始位置
    let tick2 = 170.0;
    let snapped2 = editor.snap_tick(tick2);
    assert_eq!(snapped2, 120.0);

    // 测试正好在区域边界（下一个区域的起始）
    let tick3 = 180.0;
    let snapped3 = editor.snap_tick(tick3);
    assert_eq!(snapped3, 120.0);

    // 测试下一个精度区域
    let tick4 = 240.0;
    let snapped4 = editor.snap_tick(tick4);
    assert_eq!(snapped4, 240.0);
}

/// 测试缩放限制
#[test]
fn test_zoom_limits() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;

    // 测试 X 轴最小缩放
    editor.set_zoom_x(0.0001, 0.5);
    assert!(editor.editor_state.view.zoom_x >= zoom::MIN_ZOOM_X);

    // 测试 X 轴最大缩放
    editor.set_zoom_x(100.0, 0.5);
    assert!(editor.editor_state.view.zoom_x <= zoom::MAX_ZOOM_X);

    // 测试 Y 轴最小缩放
    editor.set_zoom_y(1.0, 0.5);
    assert!(editor.editor_state.view.zoom_y >= zoom::MIN_ZOOM_Y);

    // 测试 Y 轴最大缩放
    editor.set_zoom_y(200.0, 0.5);
    assert!(editor.editor_state.view.zoom_y <= zoom::MAX_ZOOM_Y);
}
