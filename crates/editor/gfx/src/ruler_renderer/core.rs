//! 时间轴标尺渲染器 - 核心数据类型与构造方法
//!
//! 包含数据类型的定义（RulerTickInstance、RulerViewportUniform、RulerPrepareParams）、
//! RulerRenderer 的构造函数及 pipeline 相关方法。

use wgpu::util::DeviceExt;

use super::{INITIAL_CAPACITY, RulerRenderer, VERTEX_SHADER};
use crate::gpu_resource_tracker;
use crate::pipeline::RenderPipelineBuilder;
use crate::shader::create_shader_module;

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

/// 标尺准备参数
#[derive(Debug, Clone)]
pub struct RulerPrepareParams {
    pub viewport_size: (f32, f32),
    pub ruler_height: f32,
    pub keyboard_width: f32,
    pub scroll_x: f32,
    pub zoom_x: f32,
    pub ticks_per_measure: u32,
    pub ticks_per_beat: u32,
    /// PPQN 分辨率
    pub ppq: u32,
    /// 拍号变化列表 (tick, 分子, 分母)
    pub time_signatures: Vec<(u32, u8, u8)>,
}

impl RulerViewportUniform {
    pub fn from_params(params: &RulerPrepareParams) -> Self {
        Self {
            viewport_size: [params.viewport_size.0, params.viewport_size.1],
            ruler_height: params.ruler_height,
            keyboard_width: params.keyboard_width,
            scroll_x: params.scroll_x,
            zoom_x: params.zoom_x,
            ticks_per_measure: params.ticks_per_measure as f32,
            ticks_per_beat: params.ticks_per_beat as f32,
            _padding: [0.0; 2],
        }
    }
}

impl RulerRenderer {
    /// 创建新的标尺渲染器（默认带 depth attachment）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    /// 创建不带 depth attachment 的标尺渲染器（用于视频导出等纯 2D 路径）
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        let shader = create_shader_module(device, "ruler_shader", VERTEX_SHADER);

        // 创建 bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ruler_bind_group_layout"),
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
        let pipeline = RenderPipelineBuilder::new(device, "ruler_pipeline", &shader)
            .bind_group(&bind_group_layout)
            .vertex_buffer(Self::instance_buffer_layout())
            .triangle_strip()
            .alpha_blended_target(format)
            .depth_stencil(crate::constants::rendering::depth_stencil_state_for(
                needs_depth,
            ))
            .build();

        // 创建缓冲区
        let instance_buffer = Self::create_instance_buffer(device, INITIAL_CAPACITY);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ruler_viewport_uniform"),
            contents: bytemuck::cast_slice(&[RulerViewportUniform::from_params(
                &RulerPrepareParams {
                    viewport_size: (800.0, 600.0),
                    ruler_height: 30.0,
                    keyboard_width: 60.0,
                    scroll_x: 0.0,
                    zoom_x: 0.1,
                    ticks_per_measure: 1920,
                    ticks_per_beat: 480,
                    ppq: 480,
                    time_signatures: vec![(0, 4, 4)],
                },
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&viewport_buffer);

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ruler_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            instance_buffer,
            viewport_buffer,
            bind_group,
            capacity: INITIAL_CAPACITY,
            measure_color: [0.3, 0.3, 0.3, 1.0],
            beat_color: [0.5, 0.5, 0.5, 1.0],
            subdivision_color: [0.7, 0.7, 0.7, 1.0],
            background_color: [0.9, 0.9, 0.9, 1.0],
            cached_instances: Vec::new(),
            cache_valid: false,
            cache_scroll_x: 0.0,
            cache_zoom_x: 0.0,
            cache_viewport_width: 0.0,
            cache_keyboard_width: 0.0,
            cache_ruler_height: 0.0,
            cache_ticks_per_measure: 0,
            cache_ticks_per_beat: 0,
            cache_time_signatures: vec![(0, 4, 4)],
        }
    }

    /// 创建实例缓冲区
    pub(super) fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        gpu_resource_tracker::create_instance_buffer::<RulerTickInstance>(
            device,
            "ruler_instance_buffer",
            capacity,
        )
    }

    /// 实例缓冲区布局
    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RulerTickInstance>() as wgpu::BufferAddress,
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
                // tick_type
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                // tick_value
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

impl Drop for RulerRenderer {
    fn drop(&mut self) {
        gpu_resource_tracker::sub_buffer(&self.instance_buffer);
        gpu_resource_tracker::sub_buffer(&self.viewport_buffer);
    }
}

impl RulerRenderer {
    /// 设置颜色主题
    pub fn set_colors(
        &mut self,
        measure: [f32; 4],
        beat: [f32; 4],
        subdivision: [f32; 4],
        background: [f32; 4],
    ) {
        self.measure_color = measure;
        self.beat_color = beat;
        self.subdivision_color = subdivision;
        self.background_color = background;
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

    #[test]
    fn test_viewport_uniform_creation() {
        let p = RulerPrepareParams {
            viewport_size: (1920.0, 1080.0),
            ruler_height: 30.0,
            keyboard_width: 60.0,
            scroll_x: 100.0,
            zoom_x: 0.1,
            ticks_per_measure: 1920,
            ticks_per_beat: 480,
            ppq: 480,
            time_signatures: vec![(0, 4, 4)],
        };
        let uniform = RulerViewportUniform::from_params(&p);

        assert_eq!(uniform.viewport_size, [1920.0, 1080.0]);
        assert_eq!(uniform.ruler_height, 30.0);
        assert_eq!(uniform.keyboard_width, 60.0);
        assert_eq!(uniform.scroll_x, 100.0);
        assert_eq!(uniform.zoom_x, 0.1);
        assert_eq!(uniform.ticks_per_measure, 1920.0);
        assert_eq!(uniform.ticks_per_beat, 480.0);
    }
}
