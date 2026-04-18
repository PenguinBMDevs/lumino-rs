use crate::host::Host;

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

        // 小节线（垂直线）
        let ticks_per_measure = super::TICKS_PER_MEASURE as f32;
        let measure_start = (visible_tick_start / ticks_per_measure).floor() as i32;
        let measure_end = (visible_tick_end / ticks_per_measure).ceil() as i32;

        for measure in measure_start..=measure_end {
            let tick = measure as f32 * ticks_per_measure;
            let x = keyboard_width + tick * zoom_x - scroll_x;

            if x >= keyboard_width && x <= viewport_width {
                instances.push(lumino_gfx::GridLineInstance::new(
                    [x, ruler_height],
                    [x, viewport_height],
                    [0.3, 0.3, 0.3, 1.0],
                    1.0,
                ));
            }
        }

        // 拍线（垂直线）
        let ticks_per_beat = super::TICKS_PER_BEAT as f32;
        let beat_start = (visible_tick_start / ticks_per_beat).floor() as i32;
        let beat_end = (visible_tick_end / ticks_per_beat).ceil() as i32;

        for beat in beat_start..=beat_end {
            let tick = beat as f32 * ticks_per_beat;
            if tick % ticks_per_measure == 0.0 {
                continue; // 跳过小节线位置
            }
            let x = keyboard_width + tick * zoom_x - scroll_x;

            if x >= keyboard_width && x <= viewport_width {
                instances.push(lumino_gfx::GridLineInstance::new(
                    [x, ruler_height],
                    [x, viewport_height],
                    [0.2, 0.2, 0.2, 1.0],
                    0.5,
                ));
            }
        }

        // 琴键线（水平线）
        let key_start = visible_key_start.floor() as i32;
        let key_end = visible_key_end.ceil() as i32;

        for key in key_start..=key_end {
            let y = ruler_height + key as f32 * zoom_y - scroll_y;

            if y >= ruler_height && y <= viewport_height {
                // 判断是否为黑键
                let note_in_octave = key.rem_euclid(12);
                let is_black = matches!(note_in_octave, 1 | 3 | 6 | 8 | 10);

                let color = if is_black {
                    [0.15, 0.15, 0.15, 1.0]
                } else {
                    [0.1, 0.1, 0.1, 1.0]
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

        instances
    }

    /// 生成琴键实例
    pub(super) fn generate_keyboard_instances(
        &self,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_y: f32,
        zoom_y: f32,
        visible_key_count: u16,
    ) -> Vec<lumino_gfx::KeyInstance> {
        puffin::profile_function!();

        let mut instances = Vec::new();
        let max_key_index = (visible_key_count.saturating_sub(1)) as f32;

        for i in 0..visible_key_count {
            let key_index = i as isize;
            let world_y = (max_key_index - key_index as f32) * zoom_y;
            let screen_y = world_y - scroll_y + ruler_height;

            // 跳过不在视口内的键
            if screen_y + zoom_y < ruler_height || screen_y > 10000.0 {
                continue;
            }

            let note_in_octave = key_index.rem_euclid(12);
            let is_black = matches!(note_in_octave, 1 | 3 | 6 | 8 | 10);

            let color = if is_black {
                [0.2, 0.2, 0.2, 1.0]
            } else {
                [0.9, 0.9, 0.9, 1.0]
            };

            // 黑键宽度为白键的 60%
            let key_width = if is_black {
                keyboard_width * 0.6
            } else {
                keyboard_width
            };

            // 黑键水平偏移
            let x_offset = if is_black { keyboard_width * 0.4 } else { 0.0 };

            instances.push(lumino_gfx::KeyInstance::new(
                [x_offset, screen_y],
                [key_width, zoom_y],
                color,
                is_black,
                i,
            ));
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
