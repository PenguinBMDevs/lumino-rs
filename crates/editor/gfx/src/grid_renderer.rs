//! 钢琴卷帘网格渲染器
//!
//! 使用 GPU Fragment Shader 高效渲染无限网格，实现 O(1) 渲染时间。

mod camera;
mod pipeline;
mod render;

use crate::gpu_resource_tracker::TrackedBuffer;

/// Camera Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridCameraUniform {
    pub viewport_size: [f32; 2],
    pub camera_pos: [f32; 2], // (scroll_x, scroll_y)
    pub zoom: [f32; 2],       // (zoom_x, zoom_y)
    pub margins: [f32; 2],    // (keyboard_width, ruler_height)
    pub color_bg: [f32; 4],
    pub color_bg_black_key: [f32; 4],
    pub color_bar: [f32; 4],
    pub color_beat: [f32; 4],
    pub color_half_beat: [f32; 4],
    pub color_grid: [f32; 4],
    pub color_key_line: [f32; 4],
    pub ppq: f32,
    pub max_key_index: f32,
    pub canvas_offset: [f32; 2], // (offset_x, offset_y)
    pub canvas_size: [f32; 2],   // (width, height) 用于纵向头部对齐键盘顶部
    /// 当前有效的拍号变化数量
    pub time_signature_count: u32,
    /// 对齐填充，保证 vec4 数组在 WGSL uniform 中满足 16 字节对齐
    pub _padding: [u32; 1],
    /// 拍号变化列表，每个 vec4 存储 (tick, 分子, 分母, 保留)
    pub time_signatures: [[u32; 4]; 16],
}

impl GridCameraUniform {
    /// 使用 Builder 模式构造，推荐方式。
    ///
    /// ```ignore
    /// GridCameraUniform::builder()
    ///     .viewport_size(1920.0, 1080.0)
    ///     .camera_pos(100.0, 50.0)
    ///     .zoom(1.0, 0.5)
    ///     .build()
    /// ```
    pub fn builder() -> GridCameraUniformBuilder {
        GridCameraUniformBuilder::default()
    }
}

/// [`GridCameraUniform`] 的 Builder。
///
/// 20 个字段均有默认值，只需设置需要变更的字段即可。
#[derive(Debug, Clone)]
pub struct GridCameraUniformBuilder {
    viewport_size: [f32; 2],
    camera_pos: [f32; 2],
    zoom: [f32; 2],
    margins: [f32; 2],
    color_bg: [f32; 4],
    color_bg_black_key: [f32; 4],
    color_bar: [f32; 4],
    color_beat: [f32; 4],
    color_half_beat: [f32; 4],
    color_grid: [f32; 4],
    color_key_line: [f32; 4],
    ppq: f32,
    max_key_index: f32,
    canvas_offset: [f32; 2],
    canvas_size: [f32; 2],
    time_signatures: Vec<(u32, u8, u8)>,
}

/// 网格线实例（已废弃，改用 GPU infinite grid 方案）。
///
/// 兼容旧代码的占位类型，保留以避免大面积联级修改。
#[deprecated(
    since = "0.2.0",
    note = "不再使用 CPU 实例生成网格线，GridRenderer 改用 GPU infinite grid 方案"
)]
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridLineInstance {
    /// 线段起点（屏幕坐标）
    pub start: [f32; 2],
    /// 线段终点（屏幕坐标）
    pub end: [f32; 2],
    /// 线段颜色 (RGBA)
    pub color: [f32; 4],
    /// 线段宽度（像素）
    pub width: f32,
    /// 对齐填充（保持 16 字节对齐）
    pub _padding: [f32; 3],
}

impl GridLineInstance {
    /// 创建一条网格线实例。
    ///
    /// # 参数
    /// * `start` — 线段起点（屏幕坐标）
    /// * `end` — 线段终点（屏幕坐标）
    /// * `color` — RGBA 颜色
    /// * `width` — 线宽（像素）
    pub fn new(start: [f32; 2], end: [f32; 2], color: [f32; 4], width: f32) -> Self {
        Self {
            start,
            end,
            color,
            width,
            _padding: [0.0; 3],
        }
    }
}

/// 网格渲染器准备参数（聚合 GridRenderer::prepare 的 18 个参数）
#[derive(Debug, Clone)]
pub struct GridPrepareParams {
    /// 视口尺寸 [width, height]
    pub viewport_size: (f32, f32),
    /// 水平滚动（像素）
    pub scroll_x: f32,
    /// 垂直滚动（像素）
    pub scroll_y: f32,
    /// 水平缩放（像素/tick）
    pub zoom_x: f32,
    /// 垂直缩放（倍率）
    pub zoom_y: f32,
    /// 键盘分区宽度（屏宽，渲染起止均偏移量）
    pub keyboard_width: f32,
    /// 顶标尺高度（像素）
    pub ruler_height: f32,
    /// 画布背景色 (RGBA)
    pub color_bg: [f32; 4],
    /// 黑键区域背景色 (RGBA)
    pub color_bg_black_key: [f32; 4],
    /// 小节线颜色 (RGBA)
    pub color_bar: [f32; 4],
    /// 拍子线颜色 (RGBA)
    pub color_beat: [f32; 4],
    /// 半拍线颜色 (RGBA)
    pub color_half_beat: [f32; 4],
    /// 细分网格线颜色 (RGBA)
    pub color_grid: [f32; 4],
    /// 琴键分隔线颜色 (RGBA)
    pub color_key_line: [f32; 4],
    /// 分辨率（每四分音符 tick 数）
    pub ppq: f32,
    /// 最大琴键索引（决定网格的 keys 范围）
    pub max_key_index: f32,
    /// 画布水平偏移（像素）
    pub canvas_offset_x: f32,
    /// 画布垂直偏移（像素）
    pub canvas_offset_y: f32,
    /// 画布尺寸 [width, height]（纵向头部对齐键盘顶部需用）
    pub canvas_size: (f32, f32),
    /// 拍号变化列表 (tick, 分子, 分母)
    pub time_signatures: Vec<(u32, u8, u8)>,
}

/// 网格渲染器
pub struct GridRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// 视口 uniform 缓冲区
    camera_buffer: TrackedBuffer,
    /// Bind group
    bind_group: wgpu::BindGroup,
    /// 缓存的 uniform 数据（避免每帧重复构建）
    cached_uniform: Option<GridCameraUniform>,
}
