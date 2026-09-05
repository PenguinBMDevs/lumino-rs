//! Miditrail 3D 视频导出渲染器（wgpu 渲染管线实现）
//!
//! 该渲染器以 3D 透视方式渲染 MIDI 键盘与音符轨迹，结果写入离屏纹理，
//! 再由导出管线读回 CPU 并编码为视频帧。

mod aura;
mod instances;
mod key_press;
mod math;
mod pipeline;
mod quantize;
mod render_pass;
mod textures;
mod types;

/// 重导出颜色打包工具（供视频导出 waterfall/miditrail 模式使用）
pub use instances::pack_color;
pub use types::{
    MiditrailAuraInstanceGpu, MiditrailCameraGpu, MiditrailInstanceGpu, MiditrailNoteGpu,
    MiditrailUniformGpu, MiditrailViewMode,
};

use aura::{create_aura_buffers, create_aura_sampler, generate_aura_ring_data};
use instances::{
    ActiveKeys, build_aura_instances, build_key_instances, build_note_instances,
    compute_active_keys, update_key_positions,
};
use math::build_camera_uniform;
use pipeline::{
    create_aura_render_pipeline, create_bind_group_layout, create_buffers,
    create_note_render_pipeline, create_render_pipeline, create_top_note_render_pipeline,
    create_top_render_pipeline,
};
use quantize::quantize_notes_for_top;

const KEY_PRESS_SPEED_DOWN: f32 = 15.0;
const KEY_PRESS_SPEED_UP: f32 = 10.0;
const AURA_TEXTURE_SIZE: u32 = 128;

/// 3D 场景深度（tick 到 Z 坐标的映射比例）。
pub const MIDITRAIL_SCENE_DEPTH: f32 = 7.5;
/// Z 方向显示距离默认值（与场景深度相同）。
pub const MIDITRAIL_DEFAULT_Z_FAR_DISTANCE: f32 = 7.5;
/// Z 方向显示距离最大值（也是音符收集范围的最大倍数）。
pub const MIDITRAIL_MAX_Z_FAR_DISTANCE: f32 = 15.0;

/// Top 键盘去按压用的全零系数（俯视只留颜色反馈；切回 Normal 时内部
/// `key_press_factors` 仍在更新，按压动画无缝衔接）。
static ZERO_PRESS_FACTORS: [f32; 128] = [0.0; 128];

/// 3D MIDITrail 渲染器
///
/// 使用实例化立方体渲染键盘与音符，结果写入 `Rgba8Unorm` 离屏纹理。
/// Normal 与 Top 视图共用实例缓冲/纹理/深度（零第二份显存），
/// 区别仅在于相机、音符精度（Top 逐音量化对齐、永不合并）、键盘反馈
/// （Top 无按压位移、只变色）与着色器（Top flat）。
pub struct MiditrailRenderer {
    render_pipeline: wgpu::RenderPipeline,
    note_pipeline: wgpu::RenderPipeline,
    top_render_pipeline: wgpu::RenderPipeline,
    top_note_pipeline: wgpu::RenderPipeline,
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
    /// 实例构建暂存（跨帧复用，避免每帧大堆分配；渲染循环单线程独占）。
    ///
    /// 高密度导出（70 万可见音符）下 `entries` 约 39MB、`note_instances` 约 33MB，
    /// 每帧新建是毫秒级开销；复用后仅首次分配。
    scratch_entries: Vec<(u64, MiditrailInstanceGpu)>,
    scratch_notes: Vec<MiditrailInstanceGpu>,
    scratch_keys: Vec<MiditrailInstanceGpu>,
    scratch_auras: Vec<MiditrailAuraInstanceGpu>,

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
    const TOP_SHADER: &'static str = include_str!("shaders/miditrail_top.wgsl");
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
        let top_shader =
            crate::shader::create_shader_module(device, "miditrail_top_shader", Self::TOP_SHADER);
        let aura_shader =
            crate::shader::create_shader_module(device, "miditrail_aura_shader", Self::AURA_SHADER);
        let bind_group_layout = create_bind_group_layout(device);
        let render_pipeline = create_render_pipeline(device, &bind_group_layout, &shader);
        let note_pipeline = create_note_render_pipeline(device, &bind_group_layout, &shader);
        let top_render_pipeline =
            create_top_render_pipeline(device, &bind_group_layout, &top_shader);
        let top_note_pipeline =
            create_top_note_render_pipeline(device, &bind_group_layout, &top_shader);
        let aura_pipeline = create_aura_render_pipeline(device, &bind_group_layout, &aura_shader);
        let (uniform_buffer, vertex_buffer, index_buffer) =
            create_buffers(device, &Self::CUBE_VERTICES, &Self::CUBE_INDICES);
        let (aura_vertex_buffer, aura_index_buffer) = create_aura_buffers(device);
        let aura_sampler = create_aura_sampler(device);
        let aura_image_data = generate_aura_ring_data(AURA_TEXTURE_SIZE);

        Self {
            render_pipeline,
            note_pipeline,
            top_render_pipeline,
            top_note_pipeline,
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
            scratch_entries: Vec::new(),
            scratch_notes: Vec::new(),
            scratch_keys: Vec::new(),
            scratch_auras: Vec::new(),
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
    /// - `uniform` — 渲染参数（tick、尺寸、速度、视图模式等）
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

        let is_top = uniform.view_mode.is_top();
        // Top 先做逐音时间量化对齐（永不合并；Normal 路径零改动，防污染）。
        let top_notes;
        let notes: &[MiditrailNoteGpu] = if is_top {
            top_notes = quantize_notes_for_top(uniform, notes);
            &top_notes
        } else {
            notes
        };

        let mut note_instances = std::mem::take(&mut self.scratch_notes);
        note_instances.clear();
        build_note_instances(
            uniform,
            notes,
            &self.key_positions,
            &self.key_widths,
            &mut note_instances,
            &mut self.scratch_entries,
        );
        let mut key_instances = std::mem::take(&mut self.scratch_keys);
        key_instances.clear();
        // Top 键盘不要按下位移（俯视下位移丑且无意义），只保留颜色反馈：
        // 传全零 press 数组，`update_key_press_factors` 照常更新内部状态，
        // 切回 Normal 时按压动画无缝衔接。
        let press_factors: &[f32] = if is_top {
            &ZERO_PRESS_FACTORS
        } else {
            &self.key_press_factors
        };
        build_key_instances(
            uniform,
            &active_keys,
            &self.key_positions,
            &self.key_widths,
            press_factors,
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

        let mut aura_instances = std::mem::take(&mut self.scratch_auras);
        aura_instances.clear();
        if !is_top {
            // Aura 四边形在俯视下与视线垂直（零面积）天然不可见，
            // Top 直接跳过实例构建与绘制（CPU + GPU 双省）。
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
        }

        self.ensure_aura_resources(device, queue);

        let camera = build_camera_uniform(width, height, uniform.view_mode, uniform.z_far_distance);
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[camera]),
        );

        if self.bind_group.is_none() {
            self.rebuild_bind_group(device);
        }

        self.execute_render_pass(
            encoder,
            &note_instances,
            &key_instances,
            &aura_instances,
            is_top,
        );
        // 暂存 Vec 归还（保留容量，下一帧零分配复用）。
        self.scratch_notes = note_instances;
        self.scratch_keys = key_instances;
        self.scratch_auras = aura_instances;
    }

    /// 获取输出纹理引用。
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.output_texture.as_ref().map(|t| t.inner())
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
        let Some(view) = self.aura_texture_view.as_ref() else {
            debug_assert!(
                false,
                "aura 纹理应在创建 bind group 前初始化（set_aura_texture 已调用）"
            );
            return;
        };
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
