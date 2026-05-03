use super::super::types::KeyInstance;
use super::KeyboardRenderer;

impl KeyboardRenderer {
    /// 设置颜色主题
    pub fn set_colors(&mut self, white: [f32; 4], black: [f32; 4], selected: [f32; 4]) {
        self.white_key_color = white;
        self.black_key_color = black;
        self.selected_key_color = selected;
    }

    /// 生成琴键实例
    pub(super) fn generate_key_instances(
        &self,
        visible_key_count: u16,
        keyboard_width: f32,
        zoom_y: f32,
        scroll_y: f32,
        ruler_height: f32,
    ) -> Vec<KeyInstance> {
        let mut instances = Vec::with_capacity(visible_key_count as usize);
        let max_key_index = (visible_key_count.saturating_sub(1)) as f32;

        for i in 0..visible_key_count {
            let key_index = i as isize;
            let world_y = (max_key_index - key_index as f32) * zoom_y;
            let screen_y = world_y - scroll_y + ruler_height;

            // 跳过不在视口内的键
            if screen_y + zoom_y < ruler_height || screen_y > 10000.0 {
                continue;
            }

            let is_black = Self::is_key_dark(key_index);
            let color = if is_black {
                self.black_key_color
            } else {
                self.white_key_color
            };

            // 黑键宽度为白键的 60%
            let key_width = if is_black {
                keyboard_width * 0.6
            } else {
                keyboard_width
            };

            // 黑键水平偏移
            let x_offset = if is_black { keyboard_width * 0.4 } else { 0.0 };

            instances.push(KeyInstance::new(
                [x_offset, screen_y],
                [key_width, zoom_y],
                color,
                is_black,
                i,
            ));
        }

        instances
    }

    /// 判断琴键是否为黑键（12平均律）
    pub(super) fn is_key_dark(key: isize) -> bool {
        let note_in_octave = key.rem_euclid(12);
        matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
    }
}
