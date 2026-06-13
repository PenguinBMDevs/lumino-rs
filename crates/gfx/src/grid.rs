//! 网格线和标尺实例生成

use crate::constants::rendering::grid::{self, TICKS_PER_BEAT, TICKS_PER_MEASURE};
use crate::{GridLineInstance, RulerTickInstance};

/// 网格视图参数
#[derive(Debug, Clone)]
pub struct GridViewParams {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
}

/// 判断是否为黑键
pub fn is_black_key(key_index: isize) -> bool {
    let note_in_octave = key_index.rem_euclid(12);
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}

/// 自适应纵向网格线间距
fn adaptive_grid_gap(zoom_x: f32, ppq: f32) -> f32 {
    let fine_min = 4.0;
    let beat_min = 8.0;
    let bar_min = 24.0;

    if ppq / 8.0 * zoom_x >= fine_min {
        ppq / 8.0
    } else if ppq / 4.0 * zoom_x >= fine_min {
        ppq / 4.0
    } else if ppq / 2.0 * zoom_x >= fine_min {
        ppq / 2.0
    } else if ppq * zoom_x >= beat_min {
        ppq
    } else if ppq * 2.0 * zoom_x >= bar_min {
        ppq * 2.0
    } else if ppq * 4.0 * zoom_x >= bar_min {
        ppq * 4.0
    } else if ppq * 8.0 * zoom_x >= bar_min {
        ppq * 8.0
    } else if ppq * 16.0 * zoom_x >= bar_min {
        ppq * 16.0
    } else {
        ppq * 32.0
    }
}

/// 生成网格线实例
pub fn generate_grid_instances(params: &GridViewParams) -> Vec<GridLineInstance> {
    puffin::profile_function!();

    let mut instances = Vec::new();

    // 可见范围（tick 和 key）
    let visible_tick_start = params.scroll_x / params.zoom_x;
    let visible_tick_end =
        (params.scroll_x + params.viewport_width - params.keyboard_width) / params.zoom_x;
    let visible_key_start = params.scroll_y / params.zoom_y;
    let visible_key_end =
        (params.scroll_y + params.viewport_height - params.ruler_height) / params.zoom_y;

    // 琴键线（水平线，先添加使在纵向线下方渲染）
    let key_start = visible_key_start.floor() as i32;
    let key_end = visible_key_end.ceil() as i32;

    for key in key_start..=key_end {
        let y = params.ruler_height + key as f32 * params.zoom_y - params.scroll_y;

        if y >= params.ruler_height && y <= params.viewport_height {
            let is_black = is_black_key(key as isize);
            let color = if is_black {
                grid::colors::BLACK_KEY_LINE
            } else {
                grid::colors::WHITE_KEY_LINE
            };
            let width = if is_black { 0.5 } else { 0.3 };

            instances.push(GridLineInstance::new(
                [params.keyboard_width, y],
                [params.viewport_width, y],
                color,
                width,
            ));
        }
    }

    // 纵向网格线（垂直线）
    let ppq = TICKS_PER_BEAT as f32;
    let grid_gap = adaptive_grid_gap(params.zoom_x, ppq);

    let mut current_tick = (visible_tick_start / grid_gap).ceil() * grid_gap;

    while current_tick < visible_tick_end {
        let x = params.keyboard_width + current_tick * params.zoom_x - params.scroll_x;

        if x >= params.keyboard_width && x <= params.viewport_width {
            let is_measure = (current_tick % TICKS_PER_MEASURE as f32).abs() < 0.1;
            let is_beat = (current_tick % ppq).abs() < 0.1;
            let is_half_beat = (current_tick % (ppq / 2.0)).abs() < 0.1;

            let (color, width) = if is_measure {
                (grid::colors::BAR_LINE, 1.0)
            } else if is_beat {
                (grid::colors::BEAT_LINE, 0.5)
            } else if is_half_beat {
                (grid::colors::HALF_BEAT_LINE, 0.5)
            } else {
                (grid::colors::GRID_LINE, 0.3)
            };

            instances.push(GridLineInstance::new(
                [x, params.ruler_height],
                [x, params.viewport_height],
                color,
                width,
            ));
        }
        current_tick += grid_gap;
    }

    instances
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

    for beat in beat_start..=beat_end {
        let tick = beat as f32 * TICKS_PER_BEAT as f32;
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
