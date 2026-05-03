impl super::Editor {
    /// tick 转换为 x 坐标
    pub(super) fn tick_to_x(&self, tick: f32) -> f32 {
        let v = &self.editor_state.view;
        tick * v.zoom_x + v.keyboard_width - v.scroll_x
    }

    /// key 转换为 y 坐标（相对于 Canvas，包含时间轴标尺高度偏移）
    pub(super) fn key_to_y(&self, key: u16) -> f32 {
        let v = &self.editor_state.view;
        let max_key_index = (v.visible_key_count - 1) as f32;
        (max_key_index - key as f32) * v.zoom_y - v.scroll_y + v.ruler_height
    }

    /// x 坐标转换为 tick
    pub(super) fn x_to_tick(&self, x: f32) -> f32 {
        let v = &self.editor_state.view;
        (x - v.keyboard_width + v.scroll_x) / v.zoom_x
    }

    /// y 坐标转换为 key（输入为 Canvas 内的 y 坐标，需要减去时间轴标尺高度）
    pub(super) fn y_to_key(&self, y: f32) -> u16 {
        let v = &self.editor_state.view;
        let adjusted_y = y - v.ruler_height;
        let max_key_index = (v.visible_key_count - 1) as f32;
        let key_f32 = max_key_index - (adjusted_y + v.scroll_y) / v.zoom_y;
        key_f32.round().clamp(0.0, max_key_index) as u16
    }

    /// 吸附 tick 到网格
    pub(super) fn snap_tick(&self, tick: f32) -> f32 {
        let v = &self.editor_state.view;
        (tick / v.snap_precision).round() * v.snap_precision
    }
}
