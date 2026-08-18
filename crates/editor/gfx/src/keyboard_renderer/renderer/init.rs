use super::super::types::KeyboardViewportUniform;
use super::KeyboardRenderer;
use crate::gpu_resource_tracker::TrackedBuffer;
use crate::pipeline::RenderPipelineBuilder;
use crate::shader::create_shader_module;

impl KeyboardRenderer {
    /// 创建新的键盘渲染器
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = create_shader_module(device, "keyboard_shader", Self::VERTEX_SHADER);

        // 创建 bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("keyboard_bind_group_layout"),
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
        let pipeline = RenderPipelineBuilder::new(device, "keyboard_pipeline", &shader)
            .bind_group(&bind_group_layout)
            .vertex_buffer(Self::instance_buffer_layout())
            .triangle_strip()
            .alpha_blended_target(format)
            .depth_stencil(crate::constants::rendering::depth_stencil_state())
            .build();

        // 创建缓冲区
        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY);

        let viewport_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("keyboard_viewport_uniform"),
                contents: bytemuck::cast_slice(&[KeyboardViewportUniform::from_params(
                    &super::KeyboardPrepareParams {
                        viewport_size: (800.0, 600.0),
                        keyboard_width: 60.0,
                        ruler_height: 30.0,
                        scroll_y: 0.0,
                        zoom_y: 20.0,
                        visible_key_count: 128,
                    },
                )]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("keyboard_bind_group"),
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
            white_key_color: [1.0, 1.0, 1.0, 1.0],
            black_key_color: [0.2, 0.2, 0.2, 1.0],
            selected_key_color: [0.4, 0.7, 1.0, 1.0],
            cached_instances: Vec::new(),
            cache_valid: false,
            cache_scroll_y: 0.0,
            cache_zoom_y: 0.0,
            cache_visible_key_count: 0,
            cache_keyboard_width: 0.0,
            cache_ruler_height: 0.0,
        }
    }
}
