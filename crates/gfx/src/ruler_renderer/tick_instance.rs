//! 标尺刻度实例数据

/// 标尺刻度实例数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RulerTickInstance {
    /// 位置 (x, y)
    pub position: [f32; 2],
    /// 大小 (width, height)
    pub size: [f32; 2],
    /// 颜色 (r, g, b, a)
    pub color: [f32; 4],
    /// 刻度类型 (0.0 = 小节, 1.0 = 拍, 2.0 = 细分)
    pub tick_type: f32,
    /// 时间值 (tick)
    pub tick_value: f32,
    /// 填充
    pub _padding: [f32; 2],
}

impl RulerTickInstance {
    pub fn new(
        position: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        tick_type: u8,
        tick_value: f32,
    ) -> Self {
        Self {
            position,
            size,
            color,
            tick_type: tick_type as f32,
            tick_value,
            _padding: [0.0; 2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ruler_tick_instance_creation() {
        let instance = RulerTickInstance::new(
            [100.0, 0.0],
            [2.0, 30.0],
            [0.3, 0.3, 0.3, 1.0],
            0, // 小节线
            1920.0,
        );

        assert_eq!(instance.position, [100.0, 0.0]);
        assert_eq!(instance.size, [2.0, 30.0]);
        assert_eq!(instance.tick_type, 0.0);
        assert_eq!(instance.tick_value, 1920.0);
    }
}
