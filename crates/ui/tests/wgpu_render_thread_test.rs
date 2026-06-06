//! WgpuRenderThread 集成测试

use lumino_ui::{ControlCommand, RenderParams, WgpuRenderStats};

/// 测试 RenderParams 默认值
#[test]
fn test_render_params_default() {
    let params = RenderParams::default();

    assert_eq!(params.viewport_size, (800, 600));
    assert_eq!(params.scroll, (0.0, 0.0));
    assert_eq!(params.zoom, (0.1, 20.0));
    assert_eq!(params.keyboard_width, 60.0);
    assert_eq!(params.ruler_height, 30.0);
    assert_eq!(params.background_color, [0.1, 0.1, 0.1, 1.0]);
    assert!(params.grid_instances.is_empty());
    assert!(params.ruler_instances.is_empty());
    assert!(params.keyboard_instances.is_empty());
    assert_eq!(params.ticks_per_measure, 7680);
    assert_eq!(params.ticks_per_beat, 1920);
    assert_eq!(params.canvas_offset, (0.0, 0.0));
    assert_eq!(params.canvas_size, (800.0, 600.0));
    assert_eq!(params.ppq, 1920.0);
    assert_eq!(params.max_key_index, 127.0);
}

/// 测试 RenderParams 克隆
#[test]
fn test_render_params_clone() {
    let params = RenderParams {
        viewport_size: (1920, 1080),
        logical_size: (1920.0, 1080.0),
        scale_factor: 1.0,
        scroll: (100.0, 200.0),
        zoom: (0.05, 15.0),
        keyboard_width: 80.0,
        ruler_height: 40.0,
        background_color: [0.0, 0.0, 0.0, 1.0],
        color_bg: [0.1, 0.1, 0.1, 1.0],
        color_bg_black_key: [0.07, 0.07, 0.07, 1.0],
        color_bar: [0.3, 0.3, 0.3, 1.0],
        color_beat: [0.2, 0.2, 0.2, 1.0],
        color_half_beat: [0.15, 0.15, 0.15, 1.0],
        color_grid: [0.15, 0.15, 0.15, 1.0],
        color_key_line: [0.15, 0.15, 0.15, 1.0],
        grid_instances: Vec::new(),
        note_instances: Vec::new(),
        ruler_instances: Vec::new(),
        keyboard_instances: Vec::new(),
        ticks_per_measure: 960,
        ticks_per_beat: 240,
        regenerate_grid: true,
        canvas_offset: (10.0, 20.0),
        canvas_size: (1000.0, 800.0),
        ppq: 960.0,
        max_key_index: 127.0,
        is_arrangement_mode: false,
        arrangement_note_instances: Vec::new(),
        arrangement_uniform: lumino_gfx::ArrangementUniform::default(),
    };

    let cloned = params.clone();

    assert_eq!(cloned.viewport_size, params.viewport_size);
    assert_eq!(cloned.scroll, params.scroll);
    assert_eq!(cloned.zoom, params.zoom);
    assert_eq!(cloned.keyboard_width, params.keyboard_width);
    assert_eq!(cloned.ruler_height, params.ruler_height);
    assert_eq!(cloned.canvas_offset, params.canvas_offset);
    assert_eq!(cloned.canvas_size, params.canvas_size);
}

/// 测试 WgpuRenderStats 默认值
#[test]
fn test_render_stats_default() {
    let stats = WgpuRenderStats::default();

    assert_eq!(stats.frame_count, 0);
    assert_eq!(stats.last_frame_time_ms, 0.0);
    assert_eq!(stats.average_fps, 0.0);
    assert_eq!(stats.dropped_frames, 0);
    assert_eq!(stats.note_count, 0);
    assert_eq!(stats.grid_line_count, 0);
    assert_eq!(stats.key_count, 0);
    assert_eq!(stats.ruler_tick_count, 0);
}

/// 测试 WgpuRenderStats 克隆
#[test]
fn test_render_stats_clone() {
    let stats = WgpuRenderStats {
        frame_count: 1000,
        last_frame_time_ms: 16.67,
        average_fps: 60.0,
        dropped_frames: 5,
        note_count: 10000,
        grid_line_count: 500,
        key_count: 128,
        ruler_tick_count: 50,
    };

    let cloned = stats.clone();

    assert_eq!(cloned.frame_count, stats.frame_count);
    assert_eq!(cloned.last_frame_time_ms, stats.last_frame_time_ms);
    assert_eq!(cloned.average_fps, stats.average_fps);
    assert_eq!(cloned.dropped_frames, stats.dropped_frames);
    assert_eq!(cloned.note_count, stats.note_count);
    assert_eq!(cloned.grid_line_count, stats.grid_line_count);
    assert_eq!(cloned.key_count, stats.key_count);
    assert_eq!(cloned.ruler_tick_count, stats.ruler_tick_count);
}

/// 测试 ControlCommand 创建
#[test]
fn test_control_command_resize() {
    let cmd = ControlCommand::Resize {
        width: 1920,
        height: 1080,
    };

    match cmd {
        ControlCommand::Resize { width, height } => {
            assert_eq!(width, 1920);
            assert_eq!(height, 1080);
        }
        _ => panic!("Expected Resize command"),
    }
}

#[test]
fn test_control_command_shutdown() {
    let cmd = ControlCommand::Shutdown;

    match cmd {
        ControlCommand::Shutdown => {
            // 成功
        }
        _ => panic!("Expected Shutdown command"),
    }
}

/// 测试大量 RenderParams 创建性能
#[test]
fn test_render_params_creation_performance() {
    use std::time::Instant;

    let start = Instant::now();

    for _ in 0..10000 {
        let _params = RenderParams::default();
    }

    let elapsed = start.elapsed();
    println!("Created 10000 RenderParams in {:?}", elapsed);

    // 性能要求：创建 10000 个 RenderParams 应该在 10ms 以内
    assert!(
        elapsed.as_millis() < 10,
        "RenderParams creation too slow: {:?}",
        elapsed
    );
}

/// 测试 RenderParams 内存大小
#[test]
fn test_render_params_memory_size() {
    let size = std::mem::size_of::<RenderParams>();
    println!("RenderParams size: {} bytes", size);

    // RenderParams 应该相对较小，因为它包含 Vec 而不是大量数据
    assert!(size < 1000, "RenderParams too large: {} bytes", size);
}
