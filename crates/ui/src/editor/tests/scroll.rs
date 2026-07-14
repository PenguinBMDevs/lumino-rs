//! 滚动相关测试

use crate::editor::Editor;

/// 测试滚动边界
#[test]
fn test_scroll_boundaries() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.editor_state.view.total_ticks = 1000;

    // 设置一个超出范围的 scroll_x
    editor.set_scroll_x(10000.0);

    // 应该被限制在有效范围内
    assert!(editor.scroll_x() <= editor.editor_state.max_scroll.0);
    assert!(editor.scroll_x() >= 0.0);
}
