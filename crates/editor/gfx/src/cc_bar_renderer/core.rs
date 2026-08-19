//! CC 柱状条渲染器 — 核心结构体与类型定义

use crate::gpu_resource_tracker::{self, TrackedBuffer};
use crate::pipeline::RenderPipelineBuilder;
use crate::shader::create_shader_module;

/// CC / 自动化曲线实例数据 — 32 bytes
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CcBarInstance {
    /// 位置 (x, y) — 屏幕空间像素坐标，左上角
    pub position: [f32; 2],
    /// 尺寸 (width, height) — 像素
    pub size: [f32; 2],
    /// 颜色 RGBA
    pub color: [f32; 4],
    /// 圆角半径（像素）。0 表示直角矩形。
    pub corner_radius: f32,
    /// 边框宽度（像素）。0 表示无描边。
    pub border_width: f32,
}

impl CcBarInstance {
    /// 创建新的 CC 柱状条实例
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            size: [width, height],
            color,
            corner_radius: 0.0,
            border_width: 0.0,
        }
    }

    /// 创建带圆角/边框属性的实例（自动化锚点等）
    #[must_use]
    pub fn with_props(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        corner_radius: f32,
        border_width: f32,
    ) -> Self {
        Self {
            position: [x, y],
            size: [width, height],
            color,
            corner_radius,
            border_width,
        }
    }
}

/// 视口 Uniform — 8 bytes
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CcBarViewportUniform {
    /// 视口尺寸 (width, height)
    pub viewport_size: [f32; 2],
}

impl CcBarViewportUniform {
    /// 创建视口 uniform。
    ///
    /// # 参数
    /// * `viewport_width` — 视口宽度（像素）
    /// * `viewport_height` — 视口高度（像素）
    #[must_use]
    pub const fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            viewport_size: [viewport_width, viewport_height],
        }
    }
}

/// CC 柱状条渲染器
pub struct CcBarRenderer {
    /// 渲染管线
    pub(crate) pipeline: wgpu::RenderPipeline,
    /// 实例缓冲区
    pub(crate) instance_buffer: TrackedBuffer,
    /// 视口 uniform 缓冲区
    pub(crate) viewport_buffer: TrackedBuffer,
    /// Bind group
    pub(crate) bind_group: wgpu::BindGroup,
    /// 当前缓冲区容量（实例数量）
    pub(crate) capacity: usize,
}

impl CcBarRenderer {
    /// 初始缓冲区容量
    const INITIAL_CAPACITY: usize = 4096;
    /// 缓冲区扩容因子
    pub(super) const GROWTH_FACTOR: usize = 2;
    /// 顶点着色器代码
    const SHADER_SRC: &'static str = include_str!("../shaders/cc_bar.wgsl");

    /// 创建新的 CC 柱状条渲染器（默认带 depth attachment）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    /// 创建不带 depth attachment 的 CC 柱状条渲染器（用于视频导出等纯 2D 路径）
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        let shader = create_shader_module(device, "cc_bar_shader", Self::SHADER_SRC);

        // 创建 bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cc_bar_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // 创建渲染管线
        let pipeline = RenderPipelineBuilder::new(device, "cc_bar_pipeline", &shader)
            .bind_group(&bind_group_layout)
            .vertex_buffer(Self::instance_buffer_layout())
            .triangle_strip()
            .alpha_blended_target(format)
            .depth_stencil(crate::constants::rendering::depth_stencil_state_for(
                needs_depth,
            ))
            .build();

        // 创建缓冲区
        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY);

        let viewport_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("cc_bar_viewport_uniform"),
                contents: bytemuck::cast_slice(&[CcBarViewportUniform::new(800.0, 600.0)]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cc_bar_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.inner().as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            instance_buffer,
            viewport_buffer,
            bind_group,
            capacity: Self::INITIAL_CAPACITY,
        }
    }

    /// 创建实例缓冲区
    pub(super) fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> TrackedBuffer {
        gpu_resource_tracker::create_instance_buffer::<CcBarInstance>(
            device,
            "cc_bar_instance_buffer",
            capacity,
        )
    }

    /// 实例缓冲区布局
    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CcBarInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // size
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // corner_radius
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                // border_width
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

/// CC 柱状条视图参数
#[derive(Debug, Clone)]
pub struct CcBarViewParams {
    /// 面板高度（像素）
    pub panel_height: f32,
    /// 键盘分区宽度（向左偏移基准）
    pub keyboard_width: f32,
    /// 水平滚动（像素）
    pub scroll_x: f32,
    /// 水平缩放（像素/tick）
    pub zoom_x: f32,
    /// 画布水平偏移（像素）
    pub canvas_offset_x: f32,
    /// 画布垂直偏移（像素）
    pub canvas_offset_y: f32,
    /// 画布宽度（像素）
    pub canvas_size_x: f32,
    /// 画布高度（像素）
    pub canvas_size_y: f32,
    /// 自动化曲线垂直缩放（1.0 = 满量程）。
    pub value_zoom: f32,
    /// 自动化曲线垂直滚动偏移（值空间单位）。
    pub value_scroll: f32,
    /// 自动化曲线连线粗细（像素，1-10，默认 2）。
    pub line_thickness: f32,
}

/// CC 柱状条颜色配置
#[derive(Debug, Clone)]
pub struct CcBarColors {
    /// 柱状条主体颜色 (RGBA)
    pub bar_color: [f32; 4],
    /// CC 分区背景色 (RGBA)
    pub bg_color: [f32; 4],
    /// 自动化曲线手柄/节点颜色 (RGBA)
    pub handle_color: [f32; 4],
    /// 拖拽选中/抓取中的高亮颜色 (RGBA)
    pub grab_color: [f32; 4],
}

/// CC 柱状条数据点
#[derive(Debug, Clone)]
pub struct CcBarData<'a> {
    /// 力度点数据切片（柱状图/力度面板用）
    pub velocity_points: &'a [lumino_note_core::VelocityPoint],
    /// CC（控制器）点数据切片
    pub cc_points: &'a [lumino_note_core::CcPoint],
    /// 弯音（pitch bend）点数据切片
    pub bend_points: &'a [lumino_note_core::BendPoint],
    /// 可选的自动化 lane（CC / Bend 曲线模式优先使用）。
    pub automation_lane: Option<&'a lumino_note_core::AutomationLane>,
    /// 力度面板显示样式（true=曲线折线图，false=柱状图）
    pub velocity_curve_style: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cc_bar_instance_creation() {
        let instance = CcBarInstance::new(
            100.0,                 // x
            50.0,                  // y
            2.0,                   // width
            80.0,                  // height
            [0.3, 0.7, 0.9, 0.85], // color
        );

        assert_eq!(instance.position, [100.0, 50.0]);
        assert_eq!(instance.size, [2.0, 80.0]);
        assert_eq!(instance.color, [0.3, 0.7, 0.9, 0.85]);
    }

    #[test]
    fn test_viewport_uniform_creation() {
        let uniform = CcBarViewportUniform::new(1920.0, 1080.0);
        assert_eq!(uniform.viewport_size, [1920.0, 1080.0]);
    }
}
