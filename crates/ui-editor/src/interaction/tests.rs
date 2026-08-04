#[cfg(test)]
mod tests {
    use crate::*;
    use lumino_ui_core::message::EditorAction;

    #[test]
    fn test_editor_action_dispatch() {
        let mut editor = Editor::new();
        assert!(!editor.notes_changed());

        // DeletePressed 不应 panic（空 editor 下无 hover note）
        editor.handle_action(EditorAction::DeletePressed);
        assert!(!editor.notes_changed()); // 没有选中音符，notes_changed 不应变化

        // Moved 不应 panic
        editor.handle_action(EditorAction::Moved(lumino_ui_core::message::Point2::new(
            100.0, 200.0,
        )));
    }

    #[test]
    fn test_memory_breakdown_empty() {
        let editor = Editor::new();
        let mem = editor.memory_breakdown();
        assert_eq!(mem.notes_bytes, 0);
        assert_eq!(mem.track_notes_count, 0);
    }

    #[test]
    fn test_update_cursor_position() {
        let mut editor = Editor::new();
        editor.update_cursor_position(Some(iced_core::Point::new(100.0, 200.0)));
        // 不应 panic
        editor.update_cursor_position(None);
    }

    #[test]
    fn test_spatial_index_default() {
        let state = crate::SpatialIndexState::default();
        assert!(state.note_index.borrow().is_none());
        assert!(!state.note_index_dirty.get()); // 默认未脏
        assert!(state.query_cache.borrow().is_empty());
    }

    #[test]
    fn test_cache_invalidation() {
        use crate::CacheInvalidation;
        assert_eq!(
            CacheInvalidation::GRID.0 & CacheInvalidation::ALL.0,
            CacheInvalidation::GRID.0
        );
        assert_eq!(
            CacheInvalidation::NONE.0 | CacheInvalidation::KEYBOARD.0,
            CacheInvalidation::KEYBOARD.0
        );
    }

    // ── 平滑滚动方向与边界 ──

    /// 默认 view: zoom_x=0.1, zoom_y=20, total_ticks=768000,
    /// visible_key_count=128 → max_scroll=(76800, 2560)
    const DEFAULT_MAX_X: f32 = 76800.0;
    const DEFAULT_MAX_Y: f32 = 2560.0;

    #[test]
    fn test_scroll_vertical_direction_up() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;
        // 先滚到中间位置，确保减量方向可观察
        editor.editor_state.view.scroll_y = 500.0;
        editor.editor_state.view.smooth_scroll.target_y = 500.0;

        // 向上滚 → delta_y > 0 → scroll_y 应减小（显示更高音区）
        editor.handle_scrolled(0.0, 50.0);
        assert!(
            editor.editor_state.view.smooth_scroll.target_y < 500.0,
            "向上滚动应减小 scroll_y，但 target_y={} >= 500",
            editor.editor_state.view.smooth_scroll.target_y
        );
        assert!(editor.editor_state.view.smooth_scroll.active);
        // target 不应小于 0（被下界钳制）
        assert!(
            editor.editor_state.view.smooth_scroll.target_y >= 0.0,
            "target_y 不应为负，实际={}",
            editor.editor_state.view.smooth_scroll.target_y
        );
    }

    #[test]
    fn test_scroll_vertical_direction_down() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 向下滚 → delta_y < 0 → scroll_y 应增大（显示更低音区）
        editor.handle_scrolled(0.0, -50.0);
        assert!(
            editor.editor_state.view.smooth_scroll.target_y > 0.0,
            "向下滚动应增大 scroll_y，但 target_y={}",
            editor.editor_state.view.smooth_scroll.target_y
        );
        assert!(editor.editor_state.view.smooth_scroll.active);
    }

    #[test]
    fn test_scroll_horizontal_direction_right() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;
        // 先滚到中间位置，确保减量方向可观察
        editor.editor_state.view.scroll_x = 500.0;
        editor.editor_state.view.smooth_scroll.target_x = 500.0;

        // 向右滚 → delta_x > 0 → scroll_x 应减小（内容跟随手指向右，显示更前音符）
        editor.handle_scrolled(50.0, 0.0);
        assert!(
            editor.editor_state.view.smooth_scroll.target_x < 500.0,
            "向右滚动应减小 scroll_x，但 target_x={} >= 500",
            editor.editor_state.view.smooth_scroll.target_x
        );
        assert!(editor.editor_state.view.smooth_scroll.active);
    }

    #[test]
    fn test_scroll_horizontal_direction_left() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;
        // 先设到中间位置，确保增量方向可观察
        editor.editor_state.view.scroll_x = 500.0;
        editor.editor_state.view.smooth_scroll.target_x = 500.0;

        // 向左滚 → delta_x < 0 → scroll_x 应增大（内容跟随手指向左，显示更后音符）
        editor.handle_scrolled(-100.0, 0.0);
        assert!(
            editor.editor_state.view.smooth_scroll.target_x > 500.0,
            "向左滚动应增大 scroll_x，但 target_x={} <= 500",
            editor.editor_state.view.smooth_scroll.target_x
        );
    }

    #[test]
    fn test_scroll_boundary_vertical_upper() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 向下滚很大 → target_y 应被上界钳制到 max_y
        // max_y = 2560 - (500 - 24).max(0) = 2560 - 476 = 2084
        editor.handle_scrolled(0.0, -999999.0);
        let max_y = (DEFAULT_MAX_Y
            - (editor.editor_state.canvas.size_y - editor.editor_state.view.ruler_height).max(0.0))
        .max(0.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_y, max_y,
            "向下滚到极限应停在 max_y={}，实际 target_y={}",
            max_y, editor.editor_state.view.smooth_scroll.target_y
        );
    }

    #[test]
    fn test_scroll_boundary_horizontal_upper() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 向左滚很大（delta_x < 0）→ target_x 应被上界钳制到 max_x
        // max_x = 76800 - (1000 - 120).max(0) = 76800 - 880 = 75920
        editor.handle_scrolled(-999999.0, 0.0);
        let max_x = (DEFAULT_MAX_X
            - (editor.editor_state.canvas.size_x - editor.editor_state.view.keyboard_width)
                .max(0.0))
        .max(0.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_x, max_x,
            "向左滚到极限应停在 max_x={}，实际 target_x={}",
            max_x, editor.editor_state.view.smooth_scroll.target_x
        );
    }

    #[test]
    fn test_scroll_boundary_lower() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        // 从 scroll=0 向右滚（delta_x > 0）→ target 不应低于 0（下界钳制）
        editor.handle_scrolled(999999.0, 0.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_x, 0.0,
            "向右滚到极限应停在 0，实际 target_x={}",
            editor.editor_state.view.smooth_scroll.target_x
        );

        editor.handle_scrolled(0.0, 999999.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_y, 0.0,
            "向上滚到极限应停在 0，实际 target_y={}",
            editor.editor_state.view.smooth_scroll.target_y
        );
    }

    #[test]
    fn test_scroll_noop_on_zero_delta() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;

        let initial_x = editor.editor_state.view.smooth_scroll.target_x;
        let initial_y = editor.editor_state.view.smooth_scroll.target_y;

        editor.handle_scrolled(0.0, 0.0);
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_x, initial_x,
            "delta=0 不应改变 target_x"
        );
        assert_eq!(
            editor.editor_state.view.smooth_scroll.target_y, initial_y,
            "delta=0 不应改变 target_y"
        );
    }
}
