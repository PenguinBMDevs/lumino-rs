//! 键盘渲染器类型定义

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

/// 键盘视口 Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct KeyboardViewportUniform {
    /// 视口大小
    pub viewport_size: [f32; 2],
    /// 键盘宽度
    pub keyboard_width: f32,
    /// 时间轴高度
    pub ruler_height: f32,
    /// 滚动位置 Y
    pub scroll_y: f32,
    /// 缩放 Y
    pub zoom_y: f32,
    /// 可见键数量
    pub visible_key_count: f32,
    /// 填充
    pub _padding: [f32; 2],
}

impl KeyboardViewportUniform {
    /// 从准备参数创建 Uniform
    pub fn from_params(params: &super::renderer::KeyboardPrepareParams) -> Self {
        Self {
            viewport_size: [params.viewport_size.0, params.viewport_size.1],
            keyboard_width: params.keyboard_width,
            ruler_height: params.ruler_height,
            scroll_y: params.scroll_y,
            zoom_y: params.zoom_y,
            visible_key_count: params.visible_key_count as f32,
            _padding: [0.0; 2],
        }
    }
}
