use crate::constants::rendering::grid_colors;
use crate::host::Host;

/// 自适应纵向网格线间距
///
/// 根据水平缩放级别自动选择最佳网格密度，不同线类型使用不同最小间距阈值：
/// - 细网格线（32/16/8 分音符）：最小 4px
/// - 拍线（4 分音符）：最小 8px
/// - 小节线：最小 24px，逐级翻倍（2 小节 → 4 小节 → 8 小节）
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

impl Host {
    /// 生成网格线实例
    pub(super) fn generate_grid_instances(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
    ) -> Vec<lumino_gfx::GridLineInstance> {
        puffin::profile_function!();

        let mut instances = Vec::new();

        // 可见范围（tick 和 key）
        let visible_tick_start = scroll_x / zoom_x;
        let visible_tick_end = (scroll_x + viewport_width - keyboard_width) / zoom_x;
        let visible_key_start = scroll_y / zoom_y;
        let visible_key_end = (scroll_y + viewport_height - ruler_height) / zoom_y;

        // 琴键线（水平线，先添加使在纵向线下方渲染）
        let key_start = visible_key_start.floor() as i32;
        let key_end = visible_key_end.ceil() as i32;

        for key in key_start..=key_end {
            let y = ruler_height + key as f32 * zoom_y - scroll_y;

            if y >= ruler_height && y <= viewport_height {
                // 判断是否为黑键
                let is_black = Host::is_black_key(key as isize);

                let color = if is_black {
                    grid_colors::BLACK_KEY_LINE
                } else {
                    grid_colors::WHITE_KEY_LINE
                };
                let width = if is_black { 0.5 } else { 0.3 };

                instances.push(lumino_gfx::GridLineInstance::new(
                    [keyboard_width, y],
                    [viewport_width, y],
                    color,
                    width,
                ));
            }
        }

        // 纵向网格线（垂直线，后添加使在横向线上方渲染）
        // 使用自适应间距，根据缩放级别自动调整渲染密度
        let ticks_per_measure = super::TICKS_PER_MEASURE as f32;
        let ppq = super::TICKS_PER_BEAT as f32;
        let grid_gap = adaptive_grid_gap(zoom_x, ppq);

        let mut current_tick = (visible_tick_start / grid_gap).ceil() * grid_gap;

        while current_tick < visible_tick_end {
            let x = keyboard_width + current_tick * zoom_x - scroll_x;

            if x >= keyboard_width && x <= viewport_width {
                let is_measure = (current_tick % ticks_per_measure).abs() < 0.1;
                let is_beat = (current_tick % ppq).abs() < 0.1;
                let is_half_beat = (current_tick % (ppq / 2.0)).abs() < 0.1;

                let (color, width) = if is_measure {
                    (grid_colors::BAR_LINE, 1.0)
                } else if is_beat {
                    (grid_colors::BEAT_LINE, 0.5)
                } else if is_half_beat {
                    (grid_colors::HALF_BEAT_LINE, 0.5)
                } else {
                    (grid_colors::GRID_LINE, 0.3)
                };

                instances.push(lumino_gfx::GridLineInstance::new(
                    [x, ruler_height],
                    [x, viewport_height],
                    color,
                    width,
                ));
            }
            current_tick += grid_gap;
        }

        instances
    }

    /// 生成标尺实例
    pub(super) fn generate_ruler_instances(
        &self,
        viewport_width: f32,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_x: f32,
        zoom_x: f32,
        ticks_per_measure: u32,
        ticks_per_beat: u32,
    ) -> Vec<lumino_gfx::RulerTickInstance> {
        puffin::profile_function!();

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
                instances.push(lumino_gfx::RulerTickInstance::new(
                    [x, 0.0],
                    [2.0, ruler_height],
                    [0.3, 0.3, 0.3, 1.0],
                    0, // 小节线
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
                continue; // 跳过小节线位置
            }
            let x = keyboard_width + tick * zoom_x - scroll_x;

            if x >= keyboard_width && x <= viewport_width {
                instances.push(lumino_gfx::RulerTickInstance::new(
                    [x, ruler_height * 0.3],
                    [1.0, ruler_height * 0.7],
                    [0.5, 0.5, 0.5, 1.0],
                    1, // 拍线
                    tick,
                ));
            }
        }

        instances
    }
}
