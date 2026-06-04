//! 编辑器单元测试

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::constants::editor::zoom;
    use crate::toolbar::Tool;

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
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(480.0, 64, 480.0));

        // 选中第一个音符（通过 editor_state）
        editor.editor_state.interaction.selected_notes.insert(0);

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
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(480.0, 64, 480.0));

        assert_eq!(editor.editor_state.data.notes.len(), 2);

        // 删除第一个音符
        editor.delete_note_by_index(0);

        assert_eq!(editor.editor_state.data.notes.len(), 1);
        assert_eq!(editor.editor_state.data.notes[0].key, 64); // 第二个音符变成第一个
    }

    /// 测试音轨切换
    #[test]
    fn test_track_switching() {
        let mut editor = Editor::new();

        // 在当前音轨添加音符
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));

        // 切换到音轨 1
        editor.switch_to_track(1);

        assert_eq!(editor.current_track(), 1);
        assert!(editor.editor_state.data.notes.is_empty()); // 新音轨应该为空

        // 在音轨 1 添加音符
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 64, 480.0));

        // 切换回音轨 0
        editor.switch_to_track(0);

        assert_eq!(editor.editor_state.data.notes.len(), 1);
        assert_eq!(editor.editor_state.data.notes[0].key, 60); // 应该恢复原来的音符
    }

    /// 测试滚动边界
    #[test]
    fn test_scroll_boundaries() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size = iced_core::Point::new(800.0, 600.0);
        editor.editor_state.view.total_ticks = 1000;

        // 设置一个超出范围的 scroll_x
        editor.set_scroll_x(10000.0);

        // 应该被限制在有效范围内
        assert!(editor.scroll_x() <= editor.editor_state.max_scroll.x);
        assert!(editor.scroll_x() >= 0.0);
    }

    /// 测试缩放限制
    #[test]
    fn test_zoom_limits() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size = iced_core::Point::new(800.0, 600.0);

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

    /// 测试工具设置
    #[test]
    fn test_tool_setting() {
        let mut editor = Editor::new();

        // 默认应该是指针工具
        assert_eq!(editor.current_tool(), Tool::Pointer);

        // 设置为铅笔工具
        editor.set_tool(Tool::Pencil);
        assert_eq!(editor.current_tool(), Tool::Pencil);

        // 添加选中状态
        editor.editor_state.interaction.selected_notes.insert(0);
        assert_eq!(editor.selected_notes_count(), 1);

        // 切换到非指针工具应该清除选中
        editor.set_tool(Tool::Eraser);
        assert_eq!(editor.selected_notes_count(), 0);
    }

    /// 测试全选
    #[test]
    fn test_select_all_notes() {
        let mut editor = Editor::new();
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(480.0, 64, 480.0));

        editor.select_all_notes();

        assert_eq!(editor.selected_notes_count(), 2);
        assert!(editor.is_note_selected(0));
        assert!(editor.is_note_selected(1));
    }
}

/// 音符变速功能测试
#[cfg(test)]
mod speed_tests {
    use crate::editor::{Editor, Note};

    #[test]
    fn test_speed_change_empty_notes() {
        let mut editor = Editor::new();
        let modified = editor.apply_speed_change(0.5);
        assert_eq!(modified, 0);
    }

    #[test]
    fn test_speed_change_all_notes() {
        let mut editor = Editor::new();
        // 音符A: tick=0, length=480
        // 音符B: tick=600, length=240
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(600.0, 62, 240.0));

        let modified = editor.apply_speed_change(0.5);
        assert_eq!(modified, 2);

        let notes = &editor.editor_state.data.notes;
        // 以最早 tick(0) 为锚点缩放
        // A: tick'=0+(0-0)*0.5=0, length'=240
        assert!((notes[0].tick - 0.0).abs() < f32::EPSILON);
        assert!((notes[0].length - 240.0).abs() < f32::EPSILON);
        // B: tick'=0+(600-0)*0.5=300, length'=120
        assert!((notes[1].tick - 300.0).abs() < f32::EPSILON);
        assert!((notes[1].length - 120.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_speed_change_selected_notes_only() {
        let mut editor = Editor::new();
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(600.0, 62, 240.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(1200.0, 64, 120.0));

        // 只选中第 1 和第 3 个音符
        editor.editor_state.interaction.selected_notes.insert(0);
        editor.editor_state.interaction.selected_notes.insert(2);

        let modified = editor.apply_speed_change(2.0);
        assert_eq!(modified, 2);

        let notes = &editor.editor_state.data.notes;
        // A 选中: tick'=0, length'=960
        assert!((notes[0].tick - 0.0).abs() < f32::EPSILON);
        assert!((notes[0].length - 960.0).abs() < f32::EPSILON);
        // B 未选中: 不变
        assert!((notes[1].tick - 600.0).abs() < f32::EPSILON);
        assert!((notes[1].length - 240.0).abs() < f32::EPSILON);
        // C 选中: tick'=0+(1200-0)*2=2400, length'=240
        assert!((notes[2].tick - 2400.0).abs() < f32::EPSILON);
        assert!((notes[2].length - 240.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_speed_change_clamp_to_min_length() {
        let mut editor = Editor::new();
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(100.0, 60, 10.0));

        let modified = editor.apply_speed_change(0.01);
        assert_eq!(modified, 1);

        let notes = &editor.editor_state.data.notes;
        // tick 缩放: 100+(100-100)*0.01=100
        assert!((notes[0].tick - 100.0).abs() < f32::EPSILON);
        // 最小长度为 1 tick
        assert!((notes[0].length - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_speed_change_no_op_when_factor_is_one() {
        let mut editor = Editor::new();
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));

        let modified = editor.apply_speed_change(1.0);
        assert_eq!(modified, 0);

        let notes = &editor.editor_state.data.notes;
        assert!((notes[0].tick - 0.0).abs() < f32::EPSILON);
        assert!((notes[0].length - 480.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_speed_change_undo_redo() {
        let mut editor = Editor::new();
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 480.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(600.0, 62, 240.0));

        let modified = editor.apply_speed_change(0.5);
        assert_eq!(modified, 2);

        let notes = &editor.editor_state.data.notes;
        assert!((notes[0].tick - 0.0).abs() < f32::EPSILON);
        assert!((notes[0].length - 240.0).abs() < f32::EPSILON);
        assert!((notes[1].tick - 300.0).abs() < f32::EPSILON);
        assert!((notes[1].length - 120.0).abs() < f32::EPSILON);

        // 撤销
        let undo_result = editor.undo();
        assert!(undo_result);

        let notes = &editor.editor_state.data.notes;
        assert!((notes[0].tick - 0.0).abs() < f32::EPSILON);
        assert!((notes[0].length - 480.0).abs() < f32::EPSILON);
        assert!((notes[1].tick - 600.0).abs() < f32::EPSILON);
        assert!((notes[1].length - 240.0).abs() < f32::EPSILON);
    }

    /// 关键测试：尾部贴合的音符变速后仍然贴合
    #[test]
    fn test_speed_change_preserves_adjacent_notes() {
        let mut editor = Editor::new();
        // A: tick=100, length=200 → 结束于 300
        // B: tick=300, length=150 → 开始于 300
        // A 和 B 尾部贴合
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(100.0, 60, 200.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(300.0, 62, 150.0));

        let modified = editor.apply_speed_change(0.5);
        assert_eq!(modified, 2);

        let notes = &editor.editor_state.data.notes;
        // A: tick'=100+(100-100)*0.5=100, length'=100 → 结束于 200
        assert!((notes[0].tick - 100.0).abs() < f32::EPSILON);
        assert!((notes[0].length - 100.0).abs() < f32::EPSILON);
        // B: tick'=100+(300-100)*0.5=200, length'=75 → 开始于 200
        assert!((notes[1].tick - 200.0).abs() < f32::EPSILON);
        assert!((notes[1].length - 75.0).abs() < f32::EPSILON);

        // 验证贴合: A.end == B.start
        let a_end = notes[0].tick + notes[0].length;
        let b_start = notes[1].tick;
        assert!(
            (a_end - b_start).abs() < f32::EPSILON,
            "尾部贴合关系被破坏: A.end={}, B.start={}",
            a_end,
            b_start
        );
    }

    /// 验证有间隙的音符保持相对间隙比例
    #[test]
    fn test_speed_change_preserves_gap_ratio() {
        let mut editor = Editor::new();
        // A: tick=0, length=100 → 结束于 100
        // B: tick=200, length=100 → 开始于 200
        // 间隙 = 100 ticks
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(0.0, 60, 100.0));
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(200.0, 62, 100.0));

        let modified = editor.apply_speed_change(2.0);
        assert_eq!(modified, 2);

        let notes = &editor.editor_state.data.notes;
        // A: tick'=0, length'=200 → 结束于 200
        assert!((notes[0].tick - 0.0).abs() < f32::EPSILON);
        assert!((notes[0].length - 200.0).abs() < f32::EPSILON);
        // B: tick'=0+(200-0)*2=400, length'=200
        assert!((notes[1].tick - 400.0).abs() < f32::EPSILON);
        assert!((notes[1].length - 200.0).abs() < f32::EPSILON);

        // 验证间隙比例: 原始间隙=100, 缩放后间隙=200
        let original_gap = 200.0 - (0.0 + 100.0); // B.start - A.end
        let new_gap = notes[1].tick - (notes[0].tick + notes[0].length);
        assert!(
            (new_gap - original_gap * 2.0).abs() < f32::EPSILON,
            "间隙比例被破坏: 原始={}, 新={}",
            original_gap,
            new_gap
        );
    }
}
