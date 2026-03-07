impl super::Editor {
    /// tick 转换为 x 坐标
    pub(super) fn tick_to_x(&self, tick: f32) -> f32 {
        tick * self.state.zoom_x + self.state.keyboard_width - self.state.scroll_x
    }

    /// key 转换为 y 坐标
    pub(super) fn key_to_y(&self, key: u16) -> f32 {
        let max_key_index = (self.state.visible_key_count - 1) as f32;
        (max_key_index - key as f32) * self.state.zoom_y - self.state.scroll_y
    }

    /// x 坐标转换为 tick
    pub(super) fn x_to_tick(&self, x: f32) -> f32 {
        (x - self.state.keyboard_width + self.state.scroll_x) / self.state.zoom_x
    }

    /// y 坐标转换为 key
    pub(super) fn y_to_key(&self, y: f32) -> u16 {
        let max_key_index = (self.state.visible_key_count - 1) as f32;
        let key_f32 = max_key_index - (y + self.state.scroll_y) / self.state.zoom_y;
        key_f32.round().clamp(0.0, max_key_index) as u16
    }

    /// 吸附 tick 到网格
    pub(super) fn snap_tick(&self, tick: f32) -> f32 {
        (tick / self.state.snap_precision).round() * self.state.snap_precision
    }
}
