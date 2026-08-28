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
    editor.editor_state.canvas.size_x = 1280.0;
    editor.editor_state.canvas.size_y = 800.0;

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
        if (80.0..=520.0).contains(&wrapped_tick) && wrapped_tick < 500.0 {
            break;
        }
    }
    eprintln!("[DEBUG] tick after seek+wrap: {:.1}", wrapped_tick);

    assert!(
        (80.0..=300.0).contains(&wrapped_tick),
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

        let scrolled = editor.update_auto_scroll(current_tick, true);
        assert!(scrolled, "FixedIndicatorLeft 模式 auto_scroll 应始终触发");
        assert!(
            (editor.editor_state.view.scroll_x - 0.0).abs() < f32::EPSILON,
            "scroll_x 应为 0.0 (100*2-300=-100 clamp to 0)，实际 = {}",
            editor.editor_state.view.scroll_x,
        );

        let screen_x = editor.get_playback_indicator_screen_x();
        assert!(screen_x.is_some(), "指示线位置应存在");
        assert!(
            (screen_x.expect("指示线 screen_x 应为 Some，因上一行已断言 is_some") - 360.0).abs()
                < f32::EPSILON,
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

        let scrolled = editor.update_auto_scroll(current_tick, true);
        assert!(!scrolled, "ScrollingIndicator: 回绕后不应触发翻页滚动");

        let expected_indicator_x = current_tick * 2.0 - 0.0 + 60.0;
        let screen_x = editor.get_playback_indicator_screen_x();
        assert!(screen_x.is_some(), "指示线位置应存在");
        assert!(
            (screen_x
                .expect("ScrollingIndicator 指示线 screen_x 应为 Some，因上一行已断言 is_some")
                - expected_indicator_x)
                .abs()
                < 1.0,
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

        let scrolled = editor.update_auto_scroll(current_tick, true);
        assert!(!scrolled, "Off 模式 auto_scroll 不应触发");

        let expected_indicator_x = current_tick * 2.0 - 0.0 + 60.0;
        let screen_x = editor.get_playback_indicator_screen_x();
        assert!(screen_x.is_some(), "指示线位置应存在");
        assert!(
            (screen_x.expect("Off 模式指示线 screen_x 应为 Some，因上一行已断言 is_some")
                - expected_indicator_x)
                .abs()
                < 1.0,
            "Off 模式指示线应在 {:.0}px ({}*2+60)，实际 = {}",
            expected_indicator_x,
            current_tick,
            screen_x.expect("Off 模式指示线 screen_x 应为 Some，用于显示实际值"),
        );
    }

    manager.stop();
}

/// 回归测试：未处于播放状态时，自动翻页（模式2 `ScrollingIndicator`）不应触发。
///
/// 场景：播放头已越过翻页触发位置（本「应」触发翻页），但传入 `is_playing = false`。
/// 期望：返回 false 且不改变 `scroll_x`，从而不会打断用户对视图滚动的手动控制、
/// 避免视图滚动异常。同时验证播放时（`is_playing = true`）翻页仍正常工作。
#[test]
fn test_scrolling_indicator_only_triggers_when_playing() {
    let mut editor = Editor::new();

    editor.editor_state.view.zoom_x = 2.0;
    editor.editor_state.view.keyboard_width = 60.0;
    editor.editor_state.view.scroll_x = 0.0;
    editor.editor_state.canvas.size_x = 1280.0;
    editor.editor_state.canvas.size_y = 800.0;
    editor.editor_state.view.total_ticks = 2000;

    editor.set_auto_scroll_config(AutoScrollConfig {
        mode: AutoScrollMode::ScrollingIndicator,
        page_trigger_offset: 100,
        page_return_position: 100,
        ..Default::default()
    });

    // 播放头处于较大位置，使指示线越过翻页触发线
    let tick = 600.0;
    editor.playback_position = tick;

    // 非播放状态：不应触发翻页，scroll_x 保持不变（仍为 0）
    let scrolled = editor.update_auto_scroll(tick, false);
    assert!(!scrolled, "非播放状态下自动翻页不应触发");
    assert!(
        (editor.editor_state.view.scroll_x - 0.0).abs() < f32::EPSILON,
        "非播放状态下 scroll_x 不应改变，实际 = {}",
        editor.editor_state.view.scroll_x,
    );

    // 播放状态：应正常触发翻页（600*2 - 100 = 1100）
    let scrolled = editor.update_auto_scroll(tick, true);
    assert!(scrolled, "播放状态下自动翻页应触发");
    assert!(
        (editor.editor_state.view.scroll_x - 1100.0).abs() < f32::EPSILON,
        "播放状态下 scroll_x 应翻页到 1100，实际 = {}",
        editor.editor_state.view.scroll_x,
    );
}
#[test]
fn test_loop_synced_to_new_playback_manager() {
    let mut manager = PlaybackManager::new(480);
    manager.rebuild_current_track_queue();

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
        if (80.0..500.0).contains(&wrapped_tick) {
            break;
        }
    }
    assert!(
        (80.0..500.0).contains(&wrapped_tick),
        "期待回绕到 loop_start(100) 附近，实际 = {}",
        wrapped_tick,
    );

    manager.stop();
}

/// 测试 Bug 2 的完整路径：不同步循环状态时回绕不应触发
#[test]
fn test_loop_not_synced_no_wrap() {
    let mut manager = PlaybackManager::new(480);
    manager.rebuild_current_track_queue();

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
