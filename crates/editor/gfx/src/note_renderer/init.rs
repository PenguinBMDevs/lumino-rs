use super::types::CameraUniform;
use crate::epoch::GpuEpoch;
use crate::gpu_resource_tracker::TrackedBuffer;
use crate::note_renderer::NoteRenderer;

// 缓冲 / 绑定组构造拆到子模块，保持本文件 ≤600 行（REF-001）
mod buffers;

/// 着色器模块集合（横向/纵向 × 渲染/cull，按原创建顺序聚合）。
struct NoteShaderStage {
    render: wgpu::ShaderModule,
    cull: wgpu::ShaderModule,
    vertical: wgpu::ShaderModule,
    vertical_cull: wgpu::ShaderModule,
}

/// 绑定组布局集合（先渲染后 cull，保持原创建顺序）。
struct NoteBindLayouts {
    render: wgpu::BindGroupLayout,
    cull: wgpu::BindGroupLayout,
}

/// 管线集合（按原创建顺序：渲染 → cull → 点 → 直绘 → 纵向直绘）。
struct NotePipelines {
    render: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    cull: wgpu::ComputePipeline,
    vertical_cull: wgpu::ComputePipeline,
    point: Option<wgpu::RenderPipeline>,
    direct: Option<wgpu::RenderPipeline>,
    vertical_direct: Option<wgpu::RenderPipeline>,
}

/// 缓冲区与分块布局集合（按原创建顺序聚合）。
struct NoteBufferStage {
    max_capacity: usize,
    chunk_layout: super::chunk::ChunkLayout,
    indirect_template: Vec<u8>,
    gpu_note_buffer: crate::gpu_note_buffer::GpuNoteBuffer,
    visible_instance_buffer: TrackedBuffer,
    indirect_buffer: TrackedBuffer,
    viewport_buffer: TrackedBuffer,
    view_state_buffer: TrackedBuffer,
    cull_uniform_buffer: TrackedBuffer,
    cull_uniform_buffer_size: u64,
}

impl NoteRenderer {
    /// 创建新的音符渲染器（默认带 depth attachment）
    ///
    /// `epoch`：创建时所属 GPU 代次（REND-019 PR-B 登记，透传至 GpuNoteBuffer）。
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        epoch: GpuEpoch,
    ) -> Self {
        Self::new_with_depth(device, queue, format, true, epoch)
    }

    /// 创建不带 depth attachment 的音符渲染器（用于视频导出等纯 2D 路径）
    pub fn new_without_depth(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        epoch: GpuEpoch,
    ) -> Self {
        Self::new_with_depth(device, queue, format, false, epoch)
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
    /// 洋葱皮先于主音轨绘制，深度由 shader 的区域化位空间映射给出：
    /// 主音轨区 < 洋葱皮区（LessEqual 下主轨稳定覆盖洋葱皮），层间不交叠。
    ///
    /// 性能范式（照搬 wasabi 精神 + lumino 现有基础设施）：
    /// - 全量上传一次（MIDI 加载时，非每帧重写）—— 比 wasabi 每帧重写更优
    /// - GPU culling 每帧（复用 cull.wgsl 的 workgroup 批量原子 + LOD 剔除）
    /// - Indirect draw（CPU 零参与绘制提交）
    pub fn new_onion_skin(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        epoch: GpuEpoch,
    ) -> Self {
        Self::new_with_shader(
            device,
            queue,
            format,
            true,
            epoch,
            Self::ONION_SHADER,
            Self::CULL_SHADER,
        )
    }

    /// 创建不带 depth attachment 的洋葱皮音符渲染器（视频导出等纯 2D 路径）。
    ///
    /// 与 `new_onion_skin` 的唯一差异是管线**不携带** depth-stencil 状态：
    /// 视频导出的 RenderPass 无 depth attachment（`ensure_textures(.., false)`），
    /// 若管线携带 depth 状态会被 wgpu 校验层判定不兼容
    /// （`constants::is_depth_stencil_compatible`）而拒绝整条命令缓冲。
    /// `onion_note.wgsl` 的静音轨分支输出视口外退化几何（不依赖深度裁剪），
    /// 故无 depth attachment 时语义完全一致。
    pub fn new_onion_skin_without_depth(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        epoch: GpuEpoch,
    ) -> Self {
        Self::new_with_shader(
            device,
            queue,
            format,
            false,
            epoch,
            Self::ONION_SHADER,
            Self::CULL_SHADER,
        )
    }

    fn new_with_depth(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        needs_depth: bool,
        epoch: GpuEpoch,
    ) -> Self {
        Self::new_with_shader(
            device,
            queue,
            format,
            needs_depth,
            epoch,
            Self::VERTEX_SHADER,
            Self::CULL_SHADER,
        )
    }

    fn new_with_shader(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        needs_depth: bool,
        epoch: GpuEpoch,
        vertex_shader: &'static str,
        cull_shader: &'static str,
    ) -> Self {
        let is_onion = vertex_shader == Self::ONION_SHADER;
        let shaders = Self::create_shader_stage(device, vertex_shader, cull_shader, is_onion);
        let layouts = Self::create_bind_layouts(device);
        let pipelines =
            Self::create_pipelines(device, &shaders, &layouts, format, needs_depth, is_onion);
        let buffers = Self::create_buffer_stage(device, queue, epoch);
        let (render_bind_groups, cull_bind_groups) =
            Self::create_bind_groups(device, &buffers, &layouts);

        Self {
            pipeline: pipelines.render,
            vertical_pipeline: pipelines.vertical,
            cull_pipeline: pipelines.cull,
            vertical_cull_pipeline: pipelines.vertical_cull,
            gpu_note_buffer: buffers.gpu_note_buffer,
            visible_instance_buffer: buffers.visible_instance_buffer,
            indirect_buffer: buffers.indirect_buffer,
            indirect_template: buffers.indirect_template,
            external_bound: false,
            capacity: Self::INITIAL_CAPACITY,
            max_capacity: buffers.max_capacity,
            last_upload_count: 0,
            viewport_buffer: buffers.viewport_buffer,
            view_state_buffer: buffers.view_state_buffer,
            cull_uniform_buffer: buffers.cull_uniform_buffer,
            cull_uniform_buffer_size: buffers.cull_uniform_buffer_size,
            render_bind_groups,
            cull_bind_groups,
            render_bind_group_layout: layouts.render,
            cull_bind_group_layout: layouts.cull,
            chunk_layout: buffers.chunk_layout,
            point_pipeline: pipelines.point,
            direct_pipeline: pipelines.direct,
            vertical_direct_pipeline: pipelines.vertical_direct,
        }
    }

    /// 按原顺序创建横向/纵向的渲染与 cull 着色器模块。
    fn create_shader_stage(
        device: &wgpu::Device,
        vertex_shader: &'static str,
        cull_shader: &'static str,
        is_onion: bool,
    ) -> NoteShaderStage {
        let shader = crate::shader::create_shader_module(device, "note_shader", vertex_shader);
        let cull_shader = crate::shader::create_shader_module(device, "cull_shader", cull_shader);

        // 纵向转置版管线（复用同缓冲，瀑布流风格的纵向流动）
        // 洋葱皮纵向需主轨特别显示（ViewState），与普通预览音符的 note_vertical 不同
        let vertical_shader_source = if is_onion {
            Self::ONION_SHADER_VERTICAL
        } else {
            Self::VERTEX_SHADER_VERTICAL
        };
        let vertical_shader_label = if is_onion {
            "onion_note_vertical_shader"
        } else {
            "note_vertical_shader"
        };
        let vertical_shader = crate::shader::create_shader_module(
            device,
            vertical_shader_label,
            vertical_shader_source,
        );
        let vertical_cull_shader = crate::shader::create_shader_module(
            device,
            "cull_vertical_shader",
            Self::CULL_SHADER_VERTICAL,
        );

        NoteShaderStage {
            render: shader,
            cull: cull_shader,
            vertical: vertical_shader,
            vertical_cull: vertical_cull_shader,
        }
    }

    /// 按原顺序创建渲染与 cull 绑定组布局。
    fn create_bind_layouts(device: &wgpu::Device) -> NoteBindLayouts {
        // 创建渲染 bind group layout
        let render = Self::create_render_bind_group_layout(device);
        // 创建计算 bind group layout
        let cull = Self::create_cull_bind_group_layout(device);
        NoteBindLayouts { render, cull }
    }

    /// 按原顺序创建全部管线；可选管线依 is_onion / needs_depth 条件省略。
    fn create_pipelines(
        device: &wgpu::Device,
        shaders: &NoteShaderStage,
        layouts: &NoteBindLayouts,
        format: wgpu::TextureFormat,
        needs_depth: bool,
        is_onion: bool,
    ) -> NotePipelines {
        let mut pipelines =
            Self::create_core_pipelines(device, shaders, layouts, format, needs_depth, is_onion);

        // 点图元直绘管线（亚像素档位，PREF-004 P1）：仅横向洋葱皮 + 带 depth 的
        // 交互路径需要，导出无 depth 变体不建（省掉大 shader 的管线编译，启动可见）。
        pipelines.point = if is_onion && needs_depth {
            Some(Self::create_point_pipeline(
                device,
                &shaders.render,
                &layouts.render,
                format,
                needs_depth,
            ))
        } else {
            None
        };

        // VS 直绘管线（PREF-005）：**预览层也必须建**（2026-10 z-order 修复）。
        // 预览实例深度恒为 0.0（`note.wgsl` 哨兵分支），走 cull 路径时可见槽位由
        // 抢占式 `atomicAdd` 分配、顺序逐帧随机 → 重叠预览矩形「后画者胜」的赢家
        // 逐帧翻转 = 闪烁；直绘按提交序绘制 ⇒ 后来者居上且跨帧稳定。
        // 混合模式随 `is_onion`：洋葱皮不透明（early-Z），预览层 alpha 混合。
        // 导出无 depth 变体保持 `None`（`draw_direct` 内部回退 cull 路径）。
        pipelines.direct = if needs_depth {
            Some(Self::create_direct_pipeline(
                device,
                &shaders.render,
                &layouts.render,
                format,
                needs_depth,
                !is_onion,
            ))
        } else {
            None
        };

        // 纵向直绘管线（§19.4 补齐）：**只为预览层建**（`!is_onion`）—— 洋葱皮纵向
        // 仍走 cull + 可见索引路径（与本卡前既有口径一致），不为它多付一次大 shader
        // 的管线编译（启动时间敏感）。预览层纵向必须建：重叠预览矩形若走 cull
        // 随机序，纵向卷帘下同样闪烁。
        pipelines.vertical_direct = if needs_depth && !is_onion {
            Some(Self::create_direct_pipeline(
                device,
                &shaders.vertical,
                &layouts.render,
                format,
                needs_depth,
                true,
            ))
        } else {
            None
        };

        pipelines
    }

    /// 按原顺序创建横向/纵向渲染管线与 cull 计算管线，可选直绘管线留待调用方补齐。
    fn create_core_pipelines(
        device: &wgpu::Device,
        shaders: &NoteShaderStage,
        layouts: &NoteBindLayouts,
        format: wgpu::TextureFormat,
        needs_depth: bool,
        is_onion: bool,
    ) -> NotePipelines {
        // 创建渲染管线
        // `opaque`：洋葱皮/主音符层 FS 恒输出 alpha = 1.0 ⇒ 用替换写入，省掉
        // blend ROP 的读改写；预览层 `note.wgsl` 有 70% alpha 哨兵分支，保持混合。
        let pipeline = Self::create_render_pipeline(
            device,
            &shaders.render,
            &layouts.render,
            format,
            needs_depth,
            is_onion,
        );
        let vertical_pipeline = Self::create_render_pipeline(
            device,
            &shaders.vertical,
            &layouts.render,
            format,
            needs_depth,
            is_onion,
        );

        // 创建计算管线
        let cull_pipeline = Self::create_cull_pipeline(device, &shaders.cull, &layouts.cull);
        let vertical_cull_pipeline =
            Self::create_cull_pipeline(device, &shaders.vertical_cull, &layouts.cull);

        NotePipelines {
            render: pipeline,
            vertical: vertical_pipeline,
            cull: cull_pipeline,
            vertical_cull: vertical_cull_pipeline,
            point: None,
            direct: None,
            vertical_direct: None,
        }
    }

    /// 按原顺序计算容量、分块布局与模板，并创建全部缓冲区与视图状态 uniform。
    fn create_buffer_stage(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        epoch: GpuEpoch,
    ) -> NoteBufferStage {
        // 创建缓冲区
        let max_capacity = (device.limits().max_storage_buffer_binding_size as usize)
            / std::mem::size_of::<crate::NoteInstance>();
        let max_capacity = max_capacity.min(
            (device.limits().max_buffer_size as usize) / std::mem::size_of::<crate::NoteInstance>(),
        );

        // storage binding 分块布局（2GB 上限规避，详见 chunk.rs）
        let chunk_layout = super::chunk::ChunkLayout::from_limits(&device.limits());
        let slot_align = chunk_layout.slot_align;

        // 间接绘制初始化模板：每槽位一个默认 DrawIndirectArgs，逐帧复用写回
        //（原每帧 `vec![0u8; 64 × slot_align]` 堆分配，约 16KB/帧）。
        let indirect_template = Self::build_indirect_template(slot_align);

        let (
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            viewport_buffer,
            cull_uniform_buffer,
            cull_uniform_buffer_size,
        ) = Self::create_renderer_buffers(device, queue, slot_align, epoch);

        // 视图状态 uniform buffer（当前音轨 + 静音位图，切轨/静音零重传）
        let view_state_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("note_view_state_uniform"),
                contents: bytemuck::cast_slice(&[crate::note_renderer::types::ViewState::new()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        NoteBufferStage {
            max_capacity,
            chunk_layout,
            indirect_template,
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            viewport_buffer,
            view_state_buffer,
            cull_uniform_buffer,
            cull_uniform_buffer_size,
        }
    }

    /// 按原顺序创建渲染绑定组与计算绑定组。
    fn create_bind_groups(
        device: &wgpu::Device,
        buffers: &NoteBufferStage,
        layouts: &NoteBindLayouts,
    ) -> (Vec<wgpu::BindGroup>, Vec<wgpu::BindGroup>) {
        // 创建渲染 bind groups（按 chunk 分块，避免大 source buffer 整体绑定超限）
        // binding 3 = 本 chunk 的 cull uniform 槽位（chunk_start 全局基准）
        let render_bind_groups = Self::create_render_bind_groups(
            device,
            &layouts.render,
            buffers.viewport_buffer.inner(),
            buffers.view_state_buffer.inner(),
            buffers.gpu_note_buffer.buffer(),
            buffers.visible_instance_buffer.inner(),
            buffers.cull_uniform_buffer.inner(),
            &buffers.chunk_layout,
        );

        // 创建计算 bind groups（初始无实例数据，绑定切片按 buffer 容量划分）
        let cull_bind_groups = Self::build_cull_bind_groups(
            device,
            &buffers.chunk_layout,
            &buffers.gpu_note_buffer,
            buffers.visible_instance_buffer.inner(),
            buffers.indirect_buffer.inner(),
            buffers.cull_uniform_buffer.inner(),
            &buffers.cull_uniform_buffer_size,
            buffers.viewport_buffer.inner(),
            buffers.view_state_buffer.inner(),
            &layouts.cull,
        );

        (render_bind_groups, cull_bind_groups)
    }

    /// 创建所有 GPU 缓冲区（音符实例缓冲、可见索引缓冲、间接绘制缓冲、
    /// 视口/剔除 uniform 缓冲）。
    ///
    /// 2026-08-07：可见缓冲从完整 NoteInstance 改为 u32 源索引，
    /// 渲染时顶点着色器通过 storage binding 读取原数据。
    ///
    /// 间接缓冲与 cull uniform 缓冲按 `MAX_CHUNKS × slot_align` 固定槽位分配：
    /// 每 chunk 一个 `DrawIndirectArgs` / `CullUniform` 条目，chunk 数变化
    /// 无需重建 buffer（仅绑定 offset 不同）。
    fn create_renderer_buffers(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot_align: u64,
        epoch: GpuEpoch,
    ) -> (
        crate::gpu_note_buffer::GpuNoteBuffer,
        TrackedBuffer,
        TrackedBuffer,
        TrackedBuffer,
        TrackedBuffer,
        u64,
    ) {
        let gpu_note_buffer = crate::gpu_note_buffer::GpuNoteBuffer::new(device, queue, epoch);
        let visible_instance_buffer =
            Self::create_visible_index_buffer(device, Self::INITIAL_CAPACITY);

        let slot_count = super::chunk::MAX_CHUNKS as u64;
        let slot_bytes = slot_count * slot_align;
        let zeros = vec![0u8; slot_bytes as usize];
        let indirect_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("note_indirect_buffer"),
                contents: &zeros,
                usage: wgpu::BufferUsages::INDIRECT
                    | wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
            },
        );

        let viewport_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("viewport_uniform"),
                contents: bytemuck::cast_slice(&[CameraUniform::default()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        let cull_uniform_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("cull_uniform"),
                contents: &zeros,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        (
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            viewport_buffer,
            cull_uniform_buffer,
            slot_bytes,
        )
    }
}
