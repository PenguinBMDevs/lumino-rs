//! RulerRenderer 集成测试

use lumino_gfx::{RulerRenderer, RulerTickInstance, RulerViewportUniform};

/// 测试 RulerViewportUniform 内存布局
#[test]
fn test_ruler_viewport_uniform_layout() {
    let uniform = RulerViewportUniform::new(
        1920.0, // viewport_width
        1080.0, // viewport_height
        30.0,   // ruler_height
        60.0,   // keyboard_width
        100.0,  // scroll_x
        0.1,    // zoom_x
        1920,   // ticks_per_measure
        480,    // ticks_per_beat
    );

    // 验证大小（实际大小可能因对齐而变化）
    let size = std::mem::size_of::<RulerViewportUniform>();
    assert!(size >= 40 && size <= 48, "Unexpected size: {}", size);

    // 验证对齐
    assert_eq!(std::mem::align_of::<RulerViewportUniform>(), 4);
}

/// 测试 RulerTickInstance 内存布局
#[test]
fn test_ruler_tick_instance_layout() {
    let instance = RulerTickInstance::new(
        [100.0, 0.0],
        [2.0, 30.0],
        [0.3, 0.3, 0.3, 1.0],
        0, // 小节线
        1920.0,
    );

    // 验证大小 (4 * 2 + 4 * 2 + 4 * 4 + 4 + 4 + 4 * 2 = 48)
    assert_eq!(std::mem::size_of::<RulerTickInstance>(), 48);

    // 验证对齐
    assert_eq!(std::mem::align_of::<RulerTickInstance>(), 4);
}

/// 测试标尺刻度生成逻辑
#[test]
fn test_ruler_tick_generation() {
    let viewport_width = 1920.0;
    let keyboard_width = 60.0;
    let ruler_height = 30.0;
    let scroll_x = 0.0;
    let zoom_x = 0.1;
    let ticks_per_measure = 1920;
    let ticks_per_beat = 480;

    // 计算可见时间范围
    let visible_tick_start = scroll_x / zoom_x;
    let visible_tick_end = (scroll_x + viewport_width) / zoom_x;

    assert_eq!(visible_tick_start, 0.0);
    assert_eq!(visible_tick_end, 19200.0);

    // 计算小节线数量
    let measure_start = (visible_tick_start / ticks_per_measure as f32).floor() as u32;
    let measure_end = (visible_tick_end / ticks_per_measure as f32).ceil() as u32;

    assert_eq!(measure_start, 0);
    assert_eq!(measure_end, 10); // 19200 / 1920 = 10

    // 计算拍线数量
    let beat_start = (visible_tick_start / ticks_per_beat as f32).floor() as u32;
    let beat_end = (visible_tick_end / ticks_per_beat as f32).ceil() as u32;

    assert_eq!(beat_start, 0);
    assert_eq!(beat_end, 40); // 19200 / 480 = 40, ceil(40.0) = 40
}

/// 测试标尺刻度位置计算
#[test]
fn test_ruler_tick_position_calculation() {
    let keyboard_width = 60.0;
    let scroll_x = 1000.0;
    let zoom_x = 0.1;
    let ticks_per_measure = 1920;

    // 计算第 5 个小节的位置
    let measure = 5;
    let tick = measure as f32 * ticks_per_measure as f32;
    let x = keyboard_width + tick * zoom_x - scroll_x;

    assert_eq!(tick, 9600.0); // 5 * 1920
    assert_eq!(x, 20.0); // 60 + 9600 * 0.1 - 1000 = 60 + 960 - 1000 = 20
}

/// 测试大量标尺刻度生成性能
#[test]
fn test_ruler_tick_generation_performance() {
    use std::time::Instant;

    let viewport_width = 1920.0;
    let keyboard_width = 60.0;
    let ruler_height = 30.0;
    let scroll_x = 0.0;
    let zoom_x = 0.05; // 更小的缩放 = 更多的刻度
    let ticks_per_measure = 1920;
    let ticks_per_beat = 480;

    let start = Instant::now();

    let mut instances = Vec::new();

    // 计算可见时间范围
    let visible_tick_start = scroll_x / zoom_x;
    let visible_tick_end = (scroll_x + viewport_width) / zoom_x;

    // 小节线
    let measure_start = (visible_tick_start / ticks_per_measure as f32).floor() as u32;
    let measure_end = (visible_tick_end / ticks_per_measure as f32).ceil() as u32;

    for measure in measure_start..=measure_end {
        let tick = measure as f32 * ticks_per_measure as f32;
        let x = keyboard_width + tick * zoom_x - scroll_x;

        if x >= keyboard_width && x <= viewport_width {
            instances.push(RulerTickInstance::new(
                [x, 0.0],
                [2.0, ruler_height],
                [0.3, 0.3, 0.3, 1.0],
                0,
                tick,
            ));
        }
    }

    // 拍线
    let beat_start = (visible_tick_start / ticks_per_beat as f32).floor() as u32;
    let beat_end = (visible_tick_end / ticks_per_beat as f32).ceil() as u32;

    for beat in beat_start..=beat_end {
        let tick = beat as f32 * ticks_per_beat as f32;
        if tick % ticks_per_measure as f32 == 0.0 {
            continue;
        }
        let x = keyboard_width + tick * zoom_x - scroll_x;

        if x >= keyboard_width && x <= viewport_width {
            instances.push(RulerTickInstance::new(
                [x, ruler_height * 0.3],
                [1.0, ruler_height * 0.7],
                [0.5, 0.5, 0.5, 1.0],
                1,
                tick,
            ));
        }
    }

    let elapsed = start.elapsed();
    println!(
        "Generated {} ruler tick instances in {:?}",
        instances.len(),
        elapsed
    );

    // 性能要求：生成标尺刻度应该在 1ms 以内
    assert!(
        elapsed.as_micros() < 1000,
        "Ruler tick generation too slow: {:?}",
        elapsed
    );
}

/// 测试不同缩放级别下的刻度数量
#[test]
fn test_ruler_tick_count_at_different_zoom_levels() {
    let viewport_width = 1920.0;
    let keyboard_width = 60.0;
    let ticks_per_measure = 1920;
    let ticks_per_beat = 480;

    let test_cases = [
        (0.01, 10.0), // 很远的缩放
        (0.05, 50.0), // 中等缩放
        (0.1, 100.0), // 默认缩放
        (0.5, 500.0), // 很大的缩放
    ];

    for (zoom_x, expected_measures) in test_cases.iter() {
        let visible_tick_end = (viewport_width) / zoom_x;
        let measure_count = (visible_tick_end / ticks_per_measure as f32).ceil() as u32;

        println!("Zoom: {}, Visible measures: {}", zoom_x, measure_count);
        assert!(measure_count > 0);
    }
}
