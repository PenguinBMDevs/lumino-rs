//! 标尺刻度实例生成
//!
//! 注意：网格线已由 GPU 端 infinite_grid.wgsl 自动绘制，不再生成 CPU 实例。
//! `is_black_key` 保留在此处供视频导出键盘贴图生成使用。

use crate::RulerTickInstance;
use crate::constants::rendering::grid::{TICKS_PER_BEAT, TICKS_PER_MEASURE};

/// 判断是否为黑键
pub fn is_black_key(key_index: isize) -> bool {
    let note_in_octave = key_index.rem_euclid(12);
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}

/// 生成标尺实例
pub fn generate_ruler_instances(
    viewport_width: f32,
    keyboard_width: f32,
    ruler_height: f32,
    scroll_x: f32,
    zoom_x: f32,
) -> Vec<RulerTickInstance> {
    puffin::profile_function!();

    let mut instances = Vec::new();

    let visible_tick_start = scroll_x / zoom_x;
    let visible_tick_end = (scroll_x + viewport_width) / zoom_x;

    // 小节线
    let measure_start = (visible_tick_start / TICKS_PER_MEASURE as f32).floor() as u32;
    let measure_end = (visible_tick_end / TICKS_PER_MEASURE as f32).ceil() as u32;

    for measure in measure_start..=measure_end {
        let tick = measure as f32 * TICKS_PER_MEASURE as f32;
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
    let beat_start = (visible_tick_start / TICKS_PER_BEAT as f32).floor() as u32;
    let beat_end = (visible_tick_end / TICKS_PER_BEAT as f32).ceil() as u32;

    for beat_no in beat_start..=beat_end {
        let tick = beat_no as f32 * TICKS_PER_BEAT as f32;
        if tick % TICKS_PER_MEASURE as f32 == 0.0 {
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

    instances
}
