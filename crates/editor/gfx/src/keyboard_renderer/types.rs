//! 键盘渲染类型定义

/// 琴键实例数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct KeyInstance {
    /// 位置 (x, y)
    pub position: [f32; 2],
    /// 大小 (width, height)
    pub size: [f32; 2],
    /// 颜色 (r, g, b, a)
    pub color: [f32; 4],
    /// 是否黑键 (0.0 = 白键, 1.0 = 黑键)
    pub is_black: f32,
    /// 键索引
    pub key_index: f32,
    /// 填充
    pub _padding: [f32; 2],
}

impl KeyInstance {
    /// 创建琴键实例。
    ///
    /// # 参数
    /// * `position` — 位置 [x, y]
    /// * `size` — 大小 [width, height]
    /// * `color` — 颜色 (RGBA)
    /// * `is_black` — 是否为黑键
    /// * `key_index` — MIDI 键号
    pub fn new(
        position: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        is_black: bool,
        key_index: u16,
    ) -> Self {
        Self {
            position,
            size,
            color,
            is_black: if is_black { 1.0 } else { 0.0 },
            key_index: key_index as f32,
            _padding: [0.0; 2],
        }
    }
}
