use super::types::CameraUniform;
use crate::gpu_resource_tracker;
use crate::note_renderer::NoteRenderer;
use wgpu::util::DeviceExt;

impl NoteRenderer {
    /// 创建新的音符渲染器（默认带 depth attachment）
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, queue, format, true)
    }

    /// 创建不带 depth attachment 的音符渲染器（用于视频导出等纯 2D 路径）
    pub fn new_without_depth(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_depth(device, queue, format, false)
    }

    /// 创建洋葱皮音符渲染器
    ///
    /// 与主音轨 NoteRenderer 的差异：
    /// - 使用 `onion_note.wgsl`（不透明 alpha=1.0，2 像素同色加深描边）
    /// - 复用 cull.wgsl（GPU culling + indirect draw 零修改）
    ///
    /// depth 配置：与主音轨一致（needs_depth=true）
    /// 原因：encoder.rs 的 render_pass 带 depth_stencil_attachment，根据
    /// `constants::is_depth_stencil_compatible`，pipeline 必须携带匹配的
    /// depth-stencil 状态，否则 wgpu 验证层拒绝 draw call（洋葱皮不显示）。
    /// 洋葱皮先于主音轨绘制，depth=0.0 通过 LessEqual < 1.0（clear），
    /// 写入 depth=0.0 后主音轨 LessEqual 0.0<=0.0 通过并覆盖，视觉正确。
    ///
    /// 性能范式（照搬 wasabi 精神 + lumino 现有基础设施）：
    /// - 全量上传一次（MIDI 加载时，非每帧重写）—— 比 wasabi 每帧重写更优
    /// - GPU culling 每帧（复用 cull.wgsl 的 workgroup 批量原子 + LOD 剔除）
    /// - Indirect draw（CPU 零参与绘制提交）
    pub fn new_onion_skin(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_shader(
            device,
            queue,
            format,
            true,
            Self::ONION_SHADER,
            Self::CULL_SHADER,
        )
    }

    fn new_with_depth(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        Self::new_with_shader(
            device,
            queue,
            format,
            needs_depth,
            Self::VERTEX_SHADER,
            Self::CULL_SHADER,
        )
    }

    fn new_with_shader(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        needs_depth: bool,
        vertex_shader: &'static str,
        cull_shader: &'static str,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("note_shader"),
            source: wgpu::ShaderSource::Wgsl(vertex_shader.into()),
        });

        let cull_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cull_shader"),
            source: wgpu::ShaderSource::Wgsl(cull_shader.into()),
        });

        // 创建渲染 bind group layout
        let render_bind_group_layout = Self::create_render_bind_group_layout(device);

        // 创建计算 bind group layout
        let cull_bind_group_layout = Self::create_cull_bind_group_layout(device);

        // 创建渲染管线
        let pipeline = Self::create_render_pipeline(
            device,
            &shader,
            &render_bind_group_layout,
            format,
            needs_depth,
        );

        // 创建计算管线
        let cull_pipeline =
            Self::create_cull_pipeline(device, &cull_shader, &cull_bind_group_layout);

        // 创建缓冲区
        let max_capacity = (device.limits().max_storage_buffer_binding_size as usize)
            / std::mem::size_of::<crate::NoteInstance>();
        let max_capacity = max_capacity.min(
            (device.limits().max_buffer_size as usize) / std::mem::size_of::<crate::NoteInstance>(),
        );

        // storage binding 分块布局（2GB 上限规避，详见 chunk.rs）
        let chunk_layout = super::chunk::ChunkLayout::from_limits(&device.limits());
        let slot_align = chunk_layout.slot_align;

        let (
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            viewport_buffer,
            cull_uniform_buffer,
            cull_uniform_buffer_size,
        ) = Self::create_renderer_buffers(device, queue, slot_align);

        // 视图状态 uniform buffer（当前音轨 + 静音位图，切轨/静音零重传）
        let view_state_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("note_view_state_uniform"),
            contents: bytemuck::cast_slice(&[crate::note_renderer::types::ViewState::new()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&view_state_buffer);

        // 创建渲染 bind group
        let render_bind_group = Self::create_render_bind_group(
            device,
            &render_bind_group_layout,
            &viewport_buffer,
            &view_state_buffer,
        );

        // 创建计算 bind groups（初始无实例数据，绑定切片按 buffer 容量划分）
        let cull_bind_groups = Self::build_cull_bind_groups(
            device,
            &chunk_layout,
            &gpu_note_buffer,
            &visible_instance_buffer,
            &indirect_buffer,
            &cull_uniform_buffer,
            &cull_uniform_buffer_size,
            &viewport_buffer,
            &cull_bind_group_layout,
        );

        Self {
            pipeline,
            cull_pipeline,
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            capacity: Self::INITIAL_CAPACITY,
            max_capacity,
            last_upload_count: 0,
            viewport_buffer,
            view_state_buffer,
            cull_uniform_buffer,
            cull_uniform_buffer_size,
            render_bind_group,
            cull_bind_groups,
            cull_bind_group_layout,
            chunk_layout,
        }
    }

    /// 创建所有 GPU 缓冲区（音符实例缓冲、间接绘制缓冲、视口/剔除 uniform 缓冲）。
    ///
    /// 间接缓冲与 cull uniform 缓冲按 `MAX_CHUNKS × slot_align` 固定槽位分配：
    /// 每 chunk 一个 `DrawIndirectArgs` / `CullUniform` 条目，chunk 数变化
    /// 无需重建 buffer（仅绑定 offset 不同）。
    fn create_renderer_buffers(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot_align: u64,
    ) -> (
        crate::gpu_note_buffer::GpuNoteBuffer,
        wgpu::Buffer,
        wgpu::Buffer,
        wgpu::Buffer,
        wgpu::Buffer,
        u64,
    ) {
        let gpu_note_buffer = crate::gpu_note_buffer::GpuNoteBuffer::new(device, queue);
        let visible_instance_buffer =
            Self::create_instance_buffer(device, Self::INITIAL_CAPACITY, true);

        let slot_count = super::chunk::MAX_CHUNKS as u64;
        let slot_bytes = slot_count * slot_align;
        let zeros = vec![0u8; slot_bytes as usize];
        let indirect_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("note_indirect_buffer"),
            contents: &zeros,
            usage: wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        });
        gpu_resource_tracker::add_buffer(&indirect_buffer);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewport_uniform"),
            contents: bytemuck::cast_slice(&[CameraUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&viewport_buffer);

        let cull_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cull_uniform"),
            contents: &zeros,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&cull_uniform_buffer);

        (
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            viewport_buffer,
            cull_uniform_buffer,
            slot_bytes,
        )
    }

    /// 创建渲染 bind group。
    fn create_render_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        view_state_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("note_render_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: view_state_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// 创建计算 bind groups（分块切片）。
    #[allow(clippy::too_many_arguments)]
    fn build_cull_bind_groups(
        device: &wgpu::Device,
        chunk_layout: &super::chunk::ChunkLayout,
        gpu_note_buffer: &crate::gpu_note_buffer::GpuNoteBuffer,
        visible_instance_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
        cull_uniform_buffer: &wgpu::Buffer,
        cull_uniform_buffer_size: &u64,
        viewport_buffer: &wgpu::Buffer,
        cull_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Vec<wgpu::BindGroup> {
        Self::create_cull_bind_groups(
            device,
            chunk_layout,
            gpu_note_buffer.buffer(),
            visible_instance_buffer,
            indirect_buffer,
            cull_uniform_buffer,
            viewport_buffer,
            *cull_uniform_buffer_size,
            cull_bind_group_layout,
        )
    }
}
