//! 标尺视口 Uniform

/// 标尺视口 Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RulerViewportUniform {
    /// 视口大小
    pub viewport_size: [f32; 2],
    /// 标尺高度
    pub ruler_height: f32,
    /// 键盘宽度
    pub keyboard_width: f32,
    /// 滚动位置 X
    pub scroll_x: f32,
    /// 缩放 X
    pub zoom_x: f32,
    /// 每小节 tick 数
    pub ticks_per_measure: f32,
    /// 每拍 tick 数
    pub ticks_per_beat: f32,
    /// 填充
    pub _padding: [f32; 2],
}

impl RulerViewportUniform {
    pub fn new(
        viewport_width: f32,
        viewport_height: f32,
        ruler_height: f32,
        keyboard_width: f32,
        scroll_x: f32,
        zoom_x: f32,
        ticks_per_measure: u32,
        ticks_per_beat: u32,
    ) -> Self {
        Self {
            viewport_size: [viewport_width, viewport_height],
            ruler_height,
            keyboard_width,
            scroll_x,
            zoom_x,
            ticks_per_measure: ticks_per_measure as f32,
            ticks_per_beat: ticks_per_beat as f32,
            _padding: [0.0; 2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_uniform_creation() {
        let uniform = RulerViewportUniform::new(1920.0, 1080.0, 30.0, 60.0, 100.0, 0.1, 1920, 480);

        assert_eq!(uniform.viewport_size, [1920.0, 1080.0]);
        assert_eq!(uniform.ruler_height, 30.0);
        assert_eq!(uniform.keyboard_width, 60.0);
        assert_eq!(uniform.scroll_x, 100.0);
        assert_eq!(uniform.zoom_x, 0.1);
        assert_eq!(uniform.ticks_per_measure, 1920.0);
        assert_eq!(uniform.ticks_per_beat, 480.0);
    }
}
