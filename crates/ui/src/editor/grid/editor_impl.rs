//! Editor 网格线相关扩展方法

use crate::editor::Editor;

impl Editor {
    pub fn update_grid_line_instances(
        &self,
        bar_color: iced_core::Color,
        beat_color: iced_core::Color,
        half_beat_color: iced_core::Color,
        grid_color: iced_core::Color,
        key_line_color: iced_core::Color,
        instances: &mut Vec<lumino_gfx::GridLineInstance>,
    ) {
        instances.clear();
        let es = &self.editor_state;
        let view = &es.view;
        let canvas_state = &es.canvas;
        let ppq = view.ppq as f32;
        let keyboard_width = view.keyboard_width;
        let ruler_height = view.ruler_height;

        let canvas_width = canvas_state.size.x;
        let canvas_height = canvas_state.size.y;

        // === 先添加横向琴键分隔线（在纵向网格线下方渲染）===
        let start_key = view.scroll_y / view.zoom_y;
        let end_key = (view.scroll_y + canvas_height - ruler_height) / view.zoom_y;

        let mut current_key = start_key.floor() as i32;

        while (current_key as f32) < end_key {
            let screen_y = (current_key as f32 * view.zoom_y) - view.scroll_y
                + ruler_height
                + canvas_state.offset.y;

            if screen_y >= canvas_state.offset.y + ruler_height
                && screen_y <= canvas_state.offset.y + canvas_height
            {
                let is_white_key = [0, 2, 4, 5, 7, 9, 11].contains(&(current_key % 12));
                if !is_white_key {
                    let x_start = keyboard_width + canvas_state.offset.x;
                    let x_end = canvas_width + canvas_state.offset.x;
                    instances.push(lumino_gfx::GridLineInstance::new(
                        [x_start, screen_y],
                        [x_end, screen_y],
                        [
                            key_line_color.r,
                            key_line_color.g,
                            key_line_color.b,
                            key_line_color.a,
                        ],
                        1.0,
                    ));
                }
            }
            current_key += 1;
        }

        // === 后添加纵向网格线（在横向线上方渲染）===
        let measure_ticks = ppq * 4.0;
        let start_tick = view.scroll_x / view.zoom_x;
        let end_tick = (view.scroll_x + canvas_width - keyboard_width) / view.zoom_x;
        let grid_gap = super::utils::adaptive_grid_gap(view.zoom_x, ppq);

        let mut current_tick = (start_tick / grid_gap).ceil() * grid_gap;

        while current_tick < end_tick {
            let screen_x = (current_tick * view.zoom_x) - view.scroll_x
                + keyboard_width
                + canvas_state.offset.x;

            if screen_x >= canvas_state.offset.x + keyboard_width
                && screen_x <= canvas_state.offset.x + canvas_width
            {
                let is_measure = (current_tick % measure_ticks).abs() < 0.1;
                let is_beat = (current_tick % ppq).abs() < 0.1;
                let is_half_beat = (current_tick % (ppq / 2.0)).abs() < 0.1;

                let (color, width) = if is_measure {
                    ([bar_color.r, bar_color.g, bar_color.b, bar_color.a], 4.0)
                } else if is_beat {
                    (
                        [beat_color.r, beat_color.g, beat_color.b, beat_color.a],
                        1.0,
                    )
                } else if is_half_beat {
                    (
                        [
                            half_beat_color.r,
                            half_beat_color.g,
                            half_beat_color.b,
                            half_beat_color.a,
                        ],
                        1.0,
                    )
                } else {
                    (
                        [grid_color.r, grid_color.g, grid_color.b, grid_color.a],
                        1.0,
                    )
                };

                let y_start = ruler_height + canvas_state.offset.y;
                let y_end = canvas_height + canvas_state.offset.y;
                instances.push(lumino_gfx::GridLineInstance::new(
                    [screen_x, y_start],
                    [screen_x, y_end],
                    color,
                    width,
                ));
            }
            current_tick += grid_gap;
        }
    }
}
