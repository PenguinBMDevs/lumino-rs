//! Miditrail 3D 视频导出渲染器（wgpu 渲染管线实现）
//!
//! 该渲染器以 3D 透视方式渲染 MIDI 键盘与音符轨迹，结果写入离屏纹理，
//! 再由导出管线读回 CPU 并编码为视频帧。

mod aura;
mod instances;
mod key_press;
mod math;
mod pipeline;
mod render_pass;
mod types;

/// 重导出颜色打包工具（供视频导出 waterfall/miditrail 模式使用）
pub use instances::pack_color;
pub use types::{
    MiditrailAuraInstanceGpu, MiditrailCameraGpu, MiditrailInstanceGpu, MiditrailNoteGpu,
    MiditrailUniformGpu,
};

use instances::{
    ActiveKeys, build_aura_instances, build_key_instances, build_note_instances,
    compute_active_keys, update_key_positions,
};
use math::build_camera_uniform;
use pipeline::{
    create_aura_buffers, create_aura_render_pipeline, create_aura_sampler,
    create_bind_group_layout, create_buffers, create_note_render_pipeline, create_render_pipeline,
    generate_aura_ring_data,
};

const KEY_PRESS_SPEED_DOWN: f32 = 15.0;
const KEY_PRESS_SPEED_UP: f32 = 10.0;
const AURA_TEXTURE_SIZE: u32 = 128;

/// 3D 场景深度（tick 到 Z 坐标的映射比例）。
pub const MIDITRAIL_SCENE_DEPTH: f32 = 7.5;
/// Z 方向显示距离默认值（与场景深度相同）。
pub const MIDITRAIL_DEFAULT_Z_FAR_DISTANCE: f32 = 7.5;
/// Z 方向显示距离最大值（也是音符收集范围的最大倍数）。
pub const MIDITRAIL_MAX_Z_FAR_DISTANCE: f32 = 15.0;

/// 3D MIDITrail 渲染器
///
/// 使用实例化立方体渲染键盘与音符，结果写入 `Rgba8Unorm` 离屏纹理。
pub struct MiditrailRenderer {
    render_pipeline: wgpu::RenderPipeline,
    note_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,

    uniform_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    vertex_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    index_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    instance_buffer: Option<crate::gpu_resource_tracker::TrackedBuffer>,

    output_texture: Option<crate::gpu_resource_tracker::TrackedTexture>,
    output_texture_view: Option<wgpu::TextureView>,
    depth_texture: Option<crate::gpu_resource_tracker::TrackedTexture>,
    depth_texture_view: Option<wgpu::TextureView>,

    instance_capacity: usize,
    current_width: u32,
    current_height: u32,

    key_positions: Vec<f32>,
    key_widths: Vec<f32>,
    last_key_count: u32,
    key_press_factors: [f32; 128],

    // Aura 相关资源
    aura_pipeline: wgpu::RenderPipeline,
    aura_vertex_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    aura_index_buffer: crate::gpu_resource_tracker::TrackedBuffer,
    aura_instance_buffer: Option<crate::gpu_resource_tracker::TrackedBuffer>,
    aura_instance_capacity: usize,
    aura_sampler: wgpu::Sampler,
    aura_texture: Option<crate::gpu_resource_tracker::TrackedTexture>,
    aura_texture_view: Option<wgpu::TextureView>,
    aura_image_data: Vec<u8>,
    aura_resources_ready: bool,
}

impl MiditrailRenderer {
    const SHADER: &'static str = include_str!("shaders/miditrail_3d.wgsl");
    const AURA_SHADER: &'static str = include_str!("shaders/miditrail_aura.wgsl");
    // 单位立方体，每面 4 个顶点，含法线（位置 + 法线 = 6 个 f32）
    const CUBE_VERTICES: [f32; 144] = [
        // 顶面 y=1, normal (0,1,0)
        0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0,
        0.0, 1.0, 1.0, 0.0, 1.0, 0.0, // 底面 y=0, normal (0,-1,0)
        0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0,
        0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, // 正面 z=1, normal (0,0,1)
        0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0,
        0.0, 1.0, 1.0, 0.0, 0.0, 1.0, // 背面 z=0, normal (0,0,-1)
        0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 1.0, 0.0, 0.0, 0.0,
        -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0, // 左面 x=0, normal (-1,0,0)
        0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0, -1.0, 0.0,
        0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, // 右面 x=1, normal (1,0,0)
        1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0,
        1.0, 1.0, 0.0, 1.0, 0.0, 0.0,
    ];
    const CUBE_INDICES: [u16; 36] = [
        // 顶面
        0, 1, 2, 0, 2, 3, // 底面
        4, 6, 5, 4, 7, 6, // 正面
        8, 9, 10, 8, 10, 11, // 背面
        12, 14, 13, 12, 15, 14, // 左面
        16, 17, 18, 16, 18, 19, // 右面
        20, 21, 22, 20, 22, 23,
    ];

    const INITIAL_INSTANCE_CAPACITY: usize = 4096;

    /// 创建 Miditrail 渲染器。
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = crate::shader::create_shader_module(device, "miditrail_shader", Self::SHADER);
        let aura_shader =
            crate::shader::create_shader_module(device, "miditrail_aura_shader", Self::AURA_SHADER);
        let bind_group_layout = create_bind_group_layout(device);
        let render_pipeline = create_render_pipeline(device, &bind_group_layout, &shader);
        let note_pipeline = create_note_render_pipeline(device, &bind_group_layout, &shader);
        let aura_pipeline = create_aura_render_pipeline(device, &bind_group_layout, &aura_shader);
        let (uniform_buffer, vertex_buffer, index_buffer) =
            create_buffers(device, &Self::CUBE_VERTICES, &Self::CUBE_INDICES);
        let (aura_vertex_buffer, aura_index_buffer) = create_aura_buffers(device);
        let aura_sampler = create_aura_sampler(device);
        let aura_image_data = generate_aura_ring_data(AURA_TEXTURE_SIZE);

        Self {
            render_pipeline,
            note_pipeline,
            bind_group_layout,
            bind_group: None,
            uniform_buffer,
            vertex_buffer,
            index_buffer,
            instance_buffer: None,
            output_texture: None,
            output_texture_view: None,
            depth_texture: None,
            depth_texture_view: None,
            instance_capacity: 0,
            current_width: 0,
            current_height: 0,
            key_positions: Vec::new(),
            key_widths: Vec::new(),
            last_key_count: 0,
            key_press_factors: [0.0; 128],
            aura_pipeline,
            aura_vertex_buffer,
            aura_index_buffer,
            aura_instance_buffer: None,
            aura_instance_capacity: 0,
            aura_sampler,
            aura_texture: None,
            aura_texture_view: None,
            aura_image_data,
            aura_resources_ready: false,
        }
    }

    /// 渲染一帧到内部离屏纹理。
    ///
    /// # 参数
    /// - `device` — wgpu 设备
    /// - `queue` — wgpu 队列
    /// - `encoder` — 命令编码器（render pass 将追加到此 encoder）
    /// - `uniform` — 渲染参数（tick、尺寸、速度等）
    /// - `notes` — 可见音符数据切片
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &MiditrailUniformGpu,
        notes: &[MiditrailNoteGpu],
    ) {
        let width = uniform.frame_width;
        let height = uniform.frame_height;
        if width == 0 || height == 0 {
            return;
        }

        self.ensure_output_texture(device, width, height);
        update_key_positions(
            uniform.key_count,
            &mut self.last_key_count,
            &mut self.key_positions,
            &mut self.key_widths,
        );
        let active_keys = compute_active_keys(uniform.tick, notes);
        self.update_key_press_factors(&active_keys, uniform.fps);

        let mut note_instances = Vec::with_capacity(notes.len());
        build_note_instances(
            uniform,
            notes,
            &self.key_positions,
            &self.key_widths,
            &mut note_instances,
        );
        let mut key_instances = Vec::with_capacity(uniform.key_count as usize);
        build_key_instances(
            uniform,
            &active_keys,
            &self.key_positions,
            &self.key_widths,
            &self.key_press_factors,
            &mut key_instances,
        );

        let total_instances = note_instances.len() + key_instances.len();
        self.ensure_instance_buffer(device, total_instances);
        let note_bytes =
            (note_instances.len() * std::mem::size_of::<MiditrailInstanceGpu>()) as u64;
        if let Some(ref buf) = self.instance_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&note_instances));
            if !key_instances.is_empty() {
                queue.write_buffer(
                    buf.inner(),
                    note_bytes,
                    bytemuck::cast_slice(&key_instances),
                );
            }
        }

        let mut aura_instances = Vec::new();
        build_aura_instances(
            uniform,
            notes,
            &active_keys,
            &self.key_positions,
            &self.key_widths,
            &mut aura_instances,
        );
        self.ensure_aura_instance_buffer(device, aura_instances.len());
        if let Some(ref buf) = self.aura_instance_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&aura_instances));
        }

        self.ensure_aura_resources(device, queue);

        let camera = build_camera_uniform(width, height);
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[camera]),
        );

        if self.bind_group.is_none() {
            self.rebuild_bind_group(device);
        }

        self.execute_render_pass(encoder, &note_instances, &key_instances, &aura_instances);
    }

    /// 获取输出纹理引用。
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.output_texture.as_ref().map(|t| t.inner())
    }

    fn ensure_output_texture(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.current_width == width
            && self.current_height == height
            && self.output_texture.is_some()
            && self.depth_texture.is_some()
        {
            return;
        }

        self.release_textures();

        let color_texture = crate::gpu_resource_tracker::TrackedTexture::new(
            device,
            &wgpu::TextureDescriptor {
                label: Some("miditrail_output_texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
        );
        self.output_texture_view =
            Some(color_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.output_texture = Some(color_texture);

        let depth_texture = crate::gpu_resource_tracker::TrackedTexture::new(
            device,
            &wgpu::TextureDescriptor {
                label: Some("miditrail_depth_texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
        );
        self.depth_texture_view =
            Some(depth_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.depth_texture = Some(depth_texture);

        self.current_width = width;
        self.current_height = height;
        self.bind_group = None;
    }

    /// 释放纹理资源（由 [`TrackedTexture`] Drop 自动注销内存计数）
    fn release_textures(&mut self) {
        self.output_texture.take();
        self.output_texture_view.take();
        self.depth_texture.take();
        self.depth_texture_view.take();
    }

    fn ensure_instance_buffer(&mut self, device: &wgpu::Device, count: usize) {
        if count <= self.instance_capacity {
            return;
        }
        let new_cap = count
            .next_power_of_two()
            .max(Self::INITIAL_INSTANCE_CAPACITY);
        let size = (new_cap * std::mem::size_of::<MiditrailInstanceGpu>()) as u64;
        // 旧缓冲由 Option::take 触发 Drop 自动注销
        let buffer = crate::gpu_resource_tracker::TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("miditrail_instance_buffer"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        self.instance_buffer = Some(buffer);
        self.instance_capacity = new_cap;
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        // 不变式：rebuild 仅在 aura 纹理已初始化后调用（set_aura_texture 先于 render）
        let view = self.aura_texture_view.as_ref().unwrap_or_else(|| {
            unreachable!("aura 纹理应在创建 bind group 前初始化（set_aura_texture 已调用）")
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("miditrail_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.aura_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        });
        self.bind_group = Some(bind_group);
    }
}

#[cfg(test)]
mod tests;
