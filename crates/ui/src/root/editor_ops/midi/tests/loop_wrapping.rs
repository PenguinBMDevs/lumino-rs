use crate::editor::Editor;
use crate::playback::PlaybackManager;
use lumino_core::storage::config::{AutoScrollConfig, AutoScrollMode};
use std::time::{Duration, Instant};

fn wait_for_engine_start(manager: &PlaybackManager) {
    let deadline = Instant::now() + Duration::from_millis(200);
    let mut tick_before = manager.current_tick();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        let tick_now = manager.current_tick();
        if tick_now > tick_before {
            break;
        }
        tick_before = tick_now;
    }
}

/// 测试循环回绕全链路：引擎回绕 → playback_position → auto_scroll → 指示线位置
#[test]
fn test_loop_wrapping_full_pipeline_position_verification() {
    let mut manager = PlaybackManager::new(480);
    let mut editor = Editor::new();

    editor.editor_state.view.zoom_x = 2.0;
    editor.editor_state.view.keyboard_width = 60.0;
    editor.editor_state.view.scroll_x = 0.0;
    editor.editor_state.canvas.size = iced_core::Point::new(1280.0, 800.0);

    manager.set_looping(true);
    manager.set_loop_range(100.0, 500.0);

    manager.play();
    wait_for_engine_start(&manager);

    let tick_running = manager.current_tick();
    eprintln!("[DEBUG] tick after engine started: {:.1}", tick_running);
    assert!(
        tick_running > 0.0,
        "引擎应已开始播放，current_tick 应为正数，实际 = {}",
        tick_running,
    );

    // seek 到循环终点之后（600 > 500），触发回绕
    manager.seek(600.0);

    let deadline = Instant::now() + Duration::from_millis(200);
    let mut wrapped_tick = manager.current_tick();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        wrapped_tick = manager.current_tick();
        if wrapped_tick >= 80.0 && wrapped_tick <= 520.0 && wrapped_tick < 500.0 {
            break;
        }
    }
    eprintln!("[DEBUG] tick after seek+wrap: {:.1}", wrapped_tick);

    assert!(
        wrapped_tick >= 80.0 && wrapped_tick <= 300.0,
        "引擎层回绕失败：current_tick 应接近 loop_start(100)，实际 = {}",
        wrapped_tick,
    );
    assert!(
        wrapped_tick < 500.0,
        "引擎层回绕失败：tick >= loop_end(500)，实际 = {}",
        wrapped_tick,
    );

    let current_tick = wrapped_tick;

    // 验证 2: FixedIndicatorLeft 模式
    {
        editor.set_auto_scroll_config(AutoScrollConfig {
            mode: AutoScrollMode::FixedIndicatorLeft,
            fixed_indicator_position: 300,
            ..Default::default()
        });

        editor.playback_position = current_tick;

        let scrolled = editor.update_auto_scroll(current_tick);
        assert!(scrolled, "FixedIndicatorLeft 模式 auto_scroll 应始终触发");
        assert!(
            (editor.editor_state.view.scroll_x - 0.0).abs() < f32::EPSILON,
            "scroll_x 应为 0.0 (100*2-300=-100 clamp to 0)，实际 = {}",
            editor.editor_state.view.scroll_x,
        );

        let screen_x = editor.get_playback_indicator_screen_x();
        assert!(screen_x.is_some(), "指示线位置应存在");
        assert!(
            (screen_x.expect("指示线 screen_x 应为 Some，因上一行已断言 is_some") - 360.0).abs() < f32::EPSILON,
            "FixedIndicatorLeft 模式指示线应在 360px，实际 = {}",
            screen_x.expect("指示线 screen_x 应为 Some，用于显示实际值"),
        );
    }

    // 验证 3: ScrollingIndicator 模式
    {
        editor.set_auto_scroll_config(AutoScrollConfig {
            mode: AutoScrollMode::ScrollingIndicator,
            page_trigger_offset: 100,
            page_return_position: 100,
            ..Default::default()
        });

        let scrolled = editor.update_auto_scroll(current_tick);
        assert!(!scrolled, "ScrollingIndicator: 回绕后不应触发翻页滚动");

        let expected_indicator_x = current_tick * 2.0 - 0.0 + 60.0;
        let screen_x = editor.get_playback_indicator_screen_x();
        assert!(screen_x.is_some(), "指示线位置应存在");
        assert!(
            (screen_x.expect("ScrollingIndicator 指示线 screen_x 应为 Some，因上一行已断言 is_some") - expected_indicator_x).abs() < 1.0,
            "ScrollingIndicator 模式指示线应在 {:.0}px ({}*2+60)，实际 = {}",
            expected_indicator_x,
            current_tick,
            screen_x.expect("ScrollingIndicator 指示线 screen_x 应为 Some，用于显示实际值"),
        );
    }

    // 验证 4: Off 模式
    {
        editor.set_auto_scroll_config(AutoScrollConfig {
            mode: AutoScrollMode::Off,
            ..Default::default()
        });

        let scrolled = editor.update_auto_scroll(current_tick);
        assert!(!scrolled, "Off 模式 auto_scroll 不应触发");

        let expected_indicator_x = current_tick * 2.0 - 0.0 + 60.0;
        let screen_x = editor.get_playback_indicator_screen_x();
        assert!(screen_x.is_some(), "指示线位置应存在");
        assert!(
            (screen_x.expect("Off 模式指示线 screen_x 应为 Some，因上一行已断言 is_some") - expected_indicator_x).abs() < 1.0,
            "Off 模式指示线应在 {:.0}px ({}*2+60)，实际 = {}",
            expected_indicator_x,
            current_tick,
            screen_x.expect("Off 模式指示线 screen_x 应为 Some，用于显示实际值"),
        );
    }

    manager.stop();
}

/// 测试 Bug 2 场景：先开启循环再创建播放管理器，循环状态应同步到引擎
#[test]
fn test_loop_synced_to_new_playback_manager() {
    let mut manager = PlaybackManager::new(480);
    manager.set_current_track_notes(Vec::new());

    // 模拟：Editor 中循环已开启但 manager 不存在
    let mut editor = Editor::new();
    if let Some(lr) = &mut editor.loop_range {
        lr.set_range(100.0, 500.0);
        lr.enable();
    }

    // 模拟 fix: 创建 manager 后同步循环状态
    if let Some(lr) = &editor.loop_range
        && lr.enabled()
    {
        manager.set_looping(true);
        manager.set_loop_range(lr.start_tick(), lr.end_tick());
    }

    manager.play();
    wait_for_engine_start(&manager);
    assert!(manager.current_tick() > 0.0, "引擎应已开始播放");

    // seek 到循环终点后 → 应触发回绕
    manager.seek(600.0);
    let deadline = Instant::now() + Duration::from_millis(200);
    let mut wrapped_tick = manager.current_tick();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        wrapped_tick = manager.current_tick();
        if wrapped_tick >= 80.0 && wrapped_tick < 500.0 {
            break;
        }
    }
    assert!(
        wrapped_tick >= 80.0 && wrapped_tick < 500.0,
        "期待回绕到 loop_start(100) 附近，实际 = {}",
        wrapped_tick,
    );

    manager.stop();
}

/// 测试 Bug 2 的完整路径：不同步循环状态时回绕不应触发
#[test]
fn test_loop_not_synced_no_wrap() {
    let mut manager = PlaybackManager::new(480);
    manager.set_current_track_notes(Vec::new());

    manager.play();
    wait_for_engine_start(&manager);

    manager.seek(600.0);
    std::thread::sleep(Duration::from_millis(30));
    let tick_after = manager.current_tick();

    assert!(
        tick_after > 500.0,
        "未同步循环状态时不应回绕，current_tick 应 > 500，实际 = {}",
        tick_after,
    );

    manager.stop();
}
