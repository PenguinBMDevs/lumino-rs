//! Miditrail 3D 视频导出渲染器（wgpu 渲染管线实现）
//!
//! 该渲染器以 3D 透视方式渲染 MIDI 键盘与音符轨迹，结果写入离屏纹理，
//! 再由导出管线读回 CPU 并编码为视频帧。

mod instances;
mod math;
mod pipeline;
mod types;

pub use types::{MiditrailCameraGpu, MiditrailInstanceGpu, MiditrailNoteGpu, MiditrailUniformGpu};

use instances::{build_key_instances, build_note_instances, update_key_positions};
use math::build_camera_uniform;
use pipeline::{
    create_bind_group_layout, create_buffers, create_render_pipeline, create_shader_module,
};

/// 3D MIDITrail 渲染器
///
/// 使用实例化立方体渲染键盘与音符，结果写入 `Rgba8Unorm` 离屏纹理。
pub struct MiditrailRenderer {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,

    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: Option<wgpu::Buffer>,

    output_texture: Option<wgpu::Texture>,
    output_texture_view: Option<wgpu::TextureView>,
    depth_texture: Option<wgpu::Texture>,
    depth_texture_view: Option<wgpu::TextureView>,

    instance_capacity: usize,
    current_width: u32,
    current_height: u32,

    key_positions: Vec<f32>,
    key_widths: Vec<f32>,
    last_key_count: u32,
}

impl MiditrailRenderer {
    const SHADER: &'static str = include_str!("shaders/miditrail_3d.wgsl");
    // 单位立方体，每面 4 个顶点，含法线（位置 + 法线 = 6 个 f32）
    const CUBE_VERTICES: [f32; 144] = [
        // 顶面 y=1, normal (0,1,0)
        0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.0,
        0.0, 1.0, 1.0, 0.0, 1.0, 0.0, // 底面 y=0, normal (0,-1,0)
        0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0, 0.0,
        0.0, 0.0, 1.0, 0.0, -1.0, 0.0, // 正面 z=1, normal (0,0,1)
        0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0,
        0.0, 1.0, 1.0, 0.0, 0.0, 1.0, // 背面 z=0, normal (0,0,-1)
        0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 1.0, 0.0, 0.0, 0.0, -1.0,
        0.0, 1.0, 0.0, 0.0, 0.0, -1.0, // 左面 x=0, normal (-1,0,0)
        0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0, -1.0, 0.0, 0.0,
        0.0, 1.0, 0.0, -1.0, 0.0, 0.0, // 右面 x=1, normal (1,0,0)
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
        let shader = create_shader_module(device, Self::SHADER);
        let bind_group_layout = create_bind_group_layout(device);
        let render_pipeline = create_render_pipeline(device, &bind_group_layout, &shader);
        let (uniform_buffer, vertex_buffer, index_buffer) =
            create_buffers(device, &Self::CUBE_VERTICES, &Self::CUBE_INDICES);

        Self {
            render_pipeline,
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

        let mut instances = Vec::with_capacity(notes.len() + uniform.key_count as usize);
        build_note_instances(
            uniform,
            notes,
            &self.key_positions,
            &self.key_widths,
            &mut instances,
        );
        build_key_instances(
            uniform,
            notes,
            &self.key_positions,
            &self.key_widths,
            &mut instances,
        );

        self.ensure_instance_buffer(device, instances.len());
        if let Some(ref buf) = self.instance_buffer {
            queue.write_buffer(buf, 0, bytemuck::cast_slice(&instances));
        }

        let camera = build_camera_uniform(width, height);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[camera]));

        if self.bind_group.is_none() {
            self.rebuild_bind_group(device);
        }

        self.execute_render_pass(encoder, &instances);
    }

    /// 获取输出纹理引用。
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.output_texture.as_ref()
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

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
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
        });
        crate::gpu_resource_tracker::add_texture(&color_texture);
        self.output_texture_view =
            Some(color_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.output_texture = Some(color_texture);

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
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
        });
        crate::gpu_resource_tracker::add_texture(&depth_texture);
        self.depth_texture_view =
            Some(depth_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.depth_texture = Some(depth_texture);

        self.current_width = width;
        self.current_height = height;
        self.bind_group = None;
    }

    fn release_textures(&mut self) {
        if let Some(tex) = self.output_texture.take() {
            crate::gpu_resource_tracker::sub_texture(&tex);
        }
        self.output_texture_view.take();
        if let Some(tex) = self.depth_texture.take() {
            crate::gpu_resource_tracker::sub_texture(&tex);
        }
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
        if let Some(buf) = self.instance_buffer.take() {
            crate::gpu_resource_tracker::sub_buffer(&buf);
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("miditrail_instance_buffer"),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        crate::gpu_resource_tracker::add_buffer(&buffer);
        self.instance_buffer = Some(buffer);
        self.instance_capacity = new_cap;
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("miditrail_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform_buffer.as_entire_binding(),
            }],
        });
        self.bind_group = Some(bind_group);
    }

    fn execute_render_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        instances: &[MiditrailInstanceGpu],
    ) {
        let color_view = self
            .output_texture_view
            .as_ref()
            .expect("output_texture_view 应已初始化");
        let depth_view = self
            .depth_texture_view
            .as_ref()
            .expect("depth_texture_view 应已初始化");
        let bind_group = self.bind_group.as_ref().expect("bind_group 应已初始化");
        let instance_buf = self
            .instance_buffer
            .as_ref()
            .expect("instance_buffer 应已初始化");

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("miditrail_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, instance_buf.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(
                0..Self::CUBE_INDICES.len() as u32,
                0,
                0..instances.len() as u32,
            );
        }
    }
}

impl Drop for MiditrailRenderer {
    fn drop(&mut self) {
        crate::gpu_resource_tracker::sub_buffer(&self.uniform_buffer);
        crate::gpu_resource_tracker::sub_buffer(&self.vertex_buffer);
        crate::gpu_resource_tracker::sub_buffer(&self.index_buffer);
        if let Some(ref buf) = self.instance_buffer {
            crate::gpu_resource_tracker::sub_buffer(buf);
        }
        self.release_textures();
    }
}

#[cfg(test)]
mod tests;
