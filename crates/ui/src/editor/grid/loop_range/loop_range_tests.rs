use super::*;

    #[test]
    fn test_loop_range_creation() {
        let loop_range = LoopRange::new();
        assert!(!loop_range.enabled());
        assert!((loop_range.start_tick() - 0.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 1920.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_range_toggle() {
        let mut loop_range = LoopRange::new();
        assert!(!loop_range.enabled());
        loop_range.toggle();
        assert!(loop_range.enabled());
        loop_range.toggle();
        assert!(!loop_range.enabled());
    }

    #[test]
    fn test_loop_range_enable_disable() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();
        assert!(loop_range.enabled());
        loop_range.disable();
        assert!(!loop_range.enabled());
    }

    #[test]
    fn test_loop_range_resize() {
        let mut loop_range = LoopRange::new();
        loop_range.set_range(100.0, 500.0);
        assert!((loop_range.start_tick() - 100.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 500.0).abs() < f32::EPSILON);

        loop_range.update_start(200.0);
        assert!((loop_range.start_tick() - 200.0).abs() < f32::EPSILON);

        loop_range.update_end(800.0);
        assert!((loop_range.end_tick() - 800.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_contains_tick() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();

        loop_range.set_range(100.0, 500.0);

        assert!(!loop_range.contains(50.0));
        assert!(loop_range.contains(100.0));
        assert!(loop_range.contains(300.0));
        assert!(loop_range.contains(500.0));
        assert!(!loop_range.contains(600.0));

        loop_range.disable();
        assert!(!loop_range.contains(300.0));
    }

    #[test]
    fn test_loop_boundary_conditions() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();

        loop_range.set_range(-10.0, -5.0);
        assert!(loop_range.start_tick() >= 0.0);
        assert!(loop_range.end_tick() > loop_range.start_tick());

        loop_range.update_start(10000.0);
        assert!(loop_range.end_tick() > loop_range.start_tick());

        loop_range.update_end(-100.0);
        assert!(loop_range.end_tick() > loop_range.start_tick());

        loop_range.set_range(100.0, 50.0);
        assert!(loop_range.start_tick() <= loop_range.end_tick());
    }

    #[test]
    fn test_loop_length() {
        let mut loop_range = LoopRange::new();
        loop_range.set_range(100.0, 500.0);
        assert!((loop_range.length() - 400.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_to_screen_coords() {
        let mut loop_range = LoopRange::new();
        assert!(loop_range.to_screen_coords(200.0, 0.0, 1.0).is_none());

        loop_range.enable();
        loop_range.set_range(100.0, 500.0);

        let coords = loop_range.to_screen_coords(200.0, 0.0, 1.0);
        assert!(coords.is_some());
        let (start, end) = coords.unwrap();
        assert!((start - 300.0).abs() < f32::EPSILON);
        assert!((end - 700.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_move_range() {
        let mut loop_range = LoopRange::new();
        loop_range.set_range(100.0, 500.0);
        loop_range.move_range(50.0);
        assert!((loop_range.start_tick() - 150.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 550.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_move_range_negative() {
        let mut loop_range = LoopRange::new();
        loop_range.set_range(100.0, 500.0);
        loop_range.move_range(-50.0);
        assert!((loop_range.start_tick() - 50.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 450.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_is_dragging() {
        let mut loop_range = LoopRange::new();
        assert!(!loop_range.is_dragging());
        loop_range.is_dragging_start = true;
        assert!(loop_range.is_dragging());
        loop_range.is_dragging_start = false;
        loop_range.is_dragging_end = true;
        assert!(loop_range.is_dragging());
        loop_range.is_dragging_end = false;
        loop_range.is_dragging_body = true;
        assert!(loop_range.is_dragging());
    }

    #[test]
    fn test_default_implementation() {
        let loop_range = LoopRange::default();
        assert!(!loop_range.enabled());
        assert!((loop_range.start_tick() - 0.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 1920.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_body_drag_delta_aligned_to_snap_precision() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();
        loop_range.set_range(0.0, 1920.0);

        let snap_precision = 1920.0;
        let keyboard_width = 0.0;
        let scroll_x = 0.0;
        let zoom_x = 1.0;

        // press 在 screen_x=10（避开 start handle，snapped=round(10/1920)=0 → 锚点=0）
        loop_range.handle_mouse_press(10.0, keyboard_width, scroll_x, zoom_x, 40.0, snap_precision);
        assert!(loop_range.is_dragging_body);
        assert!((loop_range.drag_anchor_start_tick - 0.0).abs() < f32::EPSILON);
        assert!((loop_range.drag_anchor_mouse_tick - 0.0).abs() < f32::EPSILON);

        // 拖到 screen_x=1500 → tick=1500 → snapped=1920 → delta=1920
        // [0,1920] → [1920,3840]
        loop_range.handle_mouse_move(1500.0, keyboard_width, scroll_x, zoom_x, snap_precision);
        assert!(
            (loop_range.start_tick() - 1920.0).abs() < f32::EPSILON,
            "第一次拖动 start_tick 应为 1920，实际={}",
            loop_range.start_tick()
        );
        assert!((loop_range.end_tick() - 3840.0).abs() < f32::EPSILON);

        // 继续拖到 screen_x=5000 → tick=5000 → snapped=5760 → delta=5760
        // [1920,3840] → [5760,7680]
        loop_range.handle_mouse_move(5000.0, keyboard_width, scroll_x, zoom_x, snap_precision);
        assert!(
            (loop_range.start_tick() - 5760.0).abs() < f32::EPSILON,
            "第二次拖动 start_tick 应为 5760，实际={}",
            loop_range.start_tick()
        );
        assert!((loop_range.end_tick() - 7680.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_body_drag_precision_with_ppq_480() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();
        loop_range.set_range(0.0, 1920.0);

        let snap_precision = 480.0;
        let keyboard_width = 0.0;
        let scroll_x = 0.0;
        let zoom_x = 1.0;

        // press 在 body 区域（screen_x=10，snapped=0）
        loop_range.handle_mouse_press(10.0, keyboard_width, scroll_x, zoom_x, 40.0, snap_precision);

        // 拖到 screen_x=1500 → tick=1500 → snapped=1440 → delta=1440
        loop_range.handle_mouse_move(1500.0, keyboard_width, scroll_x, zoom_x, snap_precision);
        let start1 = loop_range.start_tick();
        assert!(
            (start1 - 1440.0).abs() < f32::EPSILON,
            "PPQ=480 第一次 start_tick 应为 1440, 实际={}",
            start1
        );

        // 继续拖到 screen_x=2500 → tick=2500 → snapped=2400 → delta=2400
        loop_range.handle_mouse_move(2500.0, keyboard_width, scroll_x, zoom_x, snap_precision);
        let start2 = loop_range.start_tick();
        assert!(
            (start2 - 2400.0).abs() < f32::EPSILON,
            "PPQ=480 第二次 start_tick 应为 2400, 实际={}",
            start2
        );
    }

    #[test]
    fn test_body_drag_delta_always_multiple_of_snap_precision() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();
        loop_range.set_range(100.0, 500.0);

        let snap_precision = 120.0;
        let keyboard_width = 0.0;
        let scroll_x = 0.0;
        let zoom_x = 1.0;

        // press 在 screen_x=300 → tick=300 → snapped=240
        loop_range.handle_mouse_press(
            300.0,
            keyboard_width,
            scroll_x,
            zoom_x,
            40.0,
            snap_precision,
        );
        let anchor_start = loop_range.drag_anchor_start_tick;

        // 随机拖拽位置，每次 start_tick - anchor_start 都应是 snap 的整数倍
        for mouse_tick in [350.0, 600.0, 777.0, 1200.0, 2500.0] {
            loop_range.handle_mouse_move(
                mouse_tick,
                keyboard_width,
                scroll_x,
                zoom_x,
                snap_precision,
            );
            let offset = loop_range.start_tick() - anchor_start;
            let snapped_offset = (offset / snap_precision).round() * snap_precision;
            assert!(
                (offset - snapped_offset).abs() < 1e-4,
                "offset {} 应为 {} 的整数倍, 偏差={}",
                offset,
                snap_precision,
                (offset - snapped_offset).abs()
            );
        }
    }

    #[test]
    fn test_body_drag_no_jitter_on_boundary_oscillation() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();
        loop_range.set_range(1000.0, 3000.0);

        let snap_precision = 480.0;
        let keyboard_width = 0.0;
        let scroll_x = 0.0;
        let zoom_x = 1.0;

        // press 在 body 区域 screen_x=1500（snapped=1440）
        loop_range.handle_mouse_press(
            1500.0,
            keyboard_width,
            scroll_x,
            zoom_x,
            40.0,
            snap_precision,
        );
        let _first_frame_start = loop_range.start_tick();

        // 第一次移动：mouse 到 screen_x=270（snapped=480），
        // raw_delta=480-1440=-960，delta=-960，new_start=1000-960=40
        loop_range.handle_mouse_move(270.0, keyboard_width, scroll_x, zoom_x, snap_precision);
        let after_first = loop_range.start_tick();

        // 之后连续多帧相同鼠标位置，start_tick 不应改变
        for _ in 0..10 {
            loop_range.handle_mouse_move(270.0, keyboard_width, scroll_x, zoom_x, snap_precision);
            assert!(
                (loop_range.start_tick() - after_first).abs() < f32::EPSILON,
                "相同鼠标位置下 start_tick 不应改变：start_tick={}，期望={}",
                loop_range.start_tick(),
                after_first
            );
        }

        // 释放后重新 press，验证锚点重建
        loop_range.handle_mouse_release();
        assert!(!loop_range.is_dragging());

        loop_range.handle_mouse_press(
            1800.0,
            keyboard_width,
            scroll_x,
            zoom_x,
            40.0,
            snap_precision,
        );
        loop_range.handle_mouse_move(1500.0, keyboard_width, scroll_x, zoom_x, snap_precision);
        let after_second_first = loop_range.start_tick();

        // 再连续多帧相同鼠标位置
        for _ in 0..10 {
            loop_range.handle_mouse_move(1500.0, keyboard_width, scroll_x, zoom_x, snap_precision);
            assert!(
                (loop_range.start_tick() - after_second_first).abs() < f32::EPSILON,
                "第二次 press 后不稳定：start_tick={}，期望={}",
                loop_range.start_tick(),
                after_second_first
            );
        }
    }

