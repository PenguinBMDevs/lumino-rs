//! 编辑器单元测试

#[cfg(test)]
mod tests {
    use super::super::*;

    /// 测试坐标转换：tick 到 x 坐标
    #[test]
    fn test_tick_to_x_conversion() {
        let editor = Editor::new();
        let tick = 100.0;
        let expected_x =
            tick * editor.state.zoom_x + editor.state.keyboard_width - editor.state.scroll_x;
        assert_eq!(editor.tick_to_x(tick), expected_x);
    }

    /// 测试坐标转换：x 到 tick
    #[test]
    fn test_x_to_tick_conversion() {
        let editor = Editor::new();
        let x = 200.0;
        let expected_tick =
            (x - editor.state.keyboard_width + editor.state.scroll_x) / editor.state.zoom_x;
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
        let key = 60u16; // 中央 C
        let max_key_index = (editor.state.visible_key_count - 1) as f32;
        let expected_y = (max_key_index - key as f32) * editor.state.zoom_y - editor.state.scroll_y;
        assert_eq!(editor.key_to_y(key), expected_y);
    }

    /// 测试 y 坐标到 key 转换
    #[test]
    fn test_y_to_key_conversion() {
        let editor = Editor::new();
        let y = 100.0;
        let key = editor.y_to_key(y);

        // 确保 key 在有效范围内
        assert!(key < editor.state.visible_key_count);
    }

    /// 测试 tick 吸附功能
    #[test]
    fn test_snap_tick() {
        let mut editor = Editor::new();
        editor.state.snap_precision = 120.0; // 1/4 音符

        // 测试向下吸附
        let tick1 = 130.0;
        let snapped1 = editor.snap_tick(tick1);
        assert_eq!(snapped1, 120.0);

        // 测试向上吸附
        let tick2 = 170.0;
        let snapped2 = editor.snap_tick(tick2);
        assert_eq!(snapped2, 240.0);

        // 测试正好在中间
        let tick3 = 180.0;
        let snapped3 = editor.snap_tick(tick3);
        assert_eq!(snapped3, 240.0);
    }

    /// 测试音符创建
    #[test]
    fn test_note_creation() {
        let tick = 0.0;
        let key = 60u16;
        let length = 480.0;

        let note = Note::new(tick, key, length);

        assert_eq!(note.tick, tick);
        assert_eq!(note.key, key);
        assert_eq!(note.length, length);
    }

    /// 测试音符选中功能
    #[test]
    fn test_note_selection() {
        let mut editor = Editor::new();

        // 添加一些音符
        editor.notes.push(Note::new(0.0, 60, 480.0));
        editor.notes.push(Note::new(480.0, 64, 480.0));

        // 选中第一个音符
        editor.selected_notes.insert(0);

        assert!(editor.is_note_selected(0));
        assert!(!editor.is_note_selected(1));
        assert_eq!(editor.selected_notes_count(), 1);

        // 清除选中
        editor.clear_selection();
        assert_eq!(editor.selected_notes_count(), 0);
    }

    /// 测试音符删除
    #[test]
    fn test_note_deletion() {
        let mut editor = Editor::new();

        // 添加音符
        editor.notes.push(Note::new(0.0, 60, 480.0));
        editor.notes.push(Note::new(480.0, 64, 480.0));

        assert_eq!(editor.notes.len(), 2);

        // 删除第一个音符
        editor.delete_note_by_index(0);

        assert_eq!(editor.notes.len(), 1);
        assert_eq!(editor.notes[0].key, 64); // 第二个音符变成第一个
    }

    /// 测试音轨切换
    #[test]
    fn test_track_switching() {
        let mut editor = Editor::new();

        // 在当前音轨添加音符
        editor.notes.push(Note::new(0.0, 60, 480.0));

        // 切换到音轨 1
        editor.switch_to_track(1);

        assert_eq!(editor.current_track(), 1);
        assert!(editor.notes.is_empty()); // 新音轨应该为空

        // 在音轨 1 添加音符
        editor.notes.push(Note::new(0.0, 64, 480.0));

        // 切换回音轨 0
        editor.switch_to_track(0);

        assert_eq!(editor.notes.len(), 1);
        assert_eq!(editor.notes[0].key, 60); // 应该恢复原来的音符
    }

    /// 测试滚动边界
    #[test]
    fn test_scroll_boundaries() {
        let mut editor = Editor::new();
        editor.canvas_size = iced_core::Size::new(800.0, 600.0);
        editor.state.total_ticks = 1000;

        // 设置一个超出范围的 scroll_x
        editor.set_scroll_x(10000.0);

        // 应该被限制在有效范围内
        assert!(editor.scroll_x() <= editor.max_scroll_x);
        assert!(editor.scroll_x() >= 0.0);
    }

    /// 测试缩放限制
    #[test]
    fn test_zoom_limits() {
        let mut editor = Editor::new();
        editor.canvas_size = iced_core::Size::new(800.0, 600.0);

        // 测试 X 轴最小缩放
        editor.set_zoom_x(0.0001, 0.5);
        assert!(editor.state.zoom_x >= constants::editor::zoom::MIN_ZOOM_X);

        // 测试 X 轴最大缩放
        editor.set_zoom_x(100.0, 0.5);
        assert!(editor.state.zoom_x <= constants::editor::zoom::MAX_ZOOM_X);

        // 测试 Y 轴最小缩放
        editor.set_zoom_y(1.0, 0.5);
        assert!(editor.state.zoom_y >= constants::editor::zoom::MIN_ZOOM_Y);

        // 测试 Y 轴最大缩放
        editor.set_zoom_y(200.0, 0.5);
        assert!(editor.state.zoom_y <= constants::editor::zoom::MAX_ZOOM_Y);
    }

    /// 测试工具设置
    #[test]
    fn test_tool_setting() {
        let mut editor = Editor::new();

        // 默认应该是指针工具
        assert_eq!(editor.current_tool(), toolbar::Tool::Pointer);

        // 设置为铅笔工具
        editor.set_tool(toolbar::Tool::Pencil);
        assert_eq!(editor.current_tool(), toolbar::Tool::Pencil);

        // 添加选中状态
        editor.selected_notes.insert(0);
        assert_eq!(editor.selected_notes_count(), 1);

        // 切换到非指针工具应该清除选中
        editor.set_tool(toolbar::Tool::Pencil);
        assert_eq!(editor.selected_notes_count(), 0);
    }
}
