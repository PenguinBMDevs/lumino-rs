use super::chunk::ChunkLayout;
use super::types::{NoteInstance, VISIBLE_INDEX_ATTRIBUTES};
use crate::{gpu_resource_tracker, note_renderer::NoteRenderer};

/// 可见索引缓冲每个元素的字节数（u32）
const VISIBLE_INDEX_SIZE: usize = std::mem::size_of::<u32>();

impl NoteRenderer {
    /// 创建可见索引缓冲区（cull 输出 / 渲染顶点输入）
    ///
    /// 2026-08-07：可见缓冲不再存完整 NoteInstance，只存 u32 源索引，
    /// 渲染时从 all_instances storage buffer 读取原数据，显存占用降为 1/4。
    pub(super) fn create_visible_index_buffer(
        device: &wgpu::Device,
        capacity: usize,
    ) -> wgpu::Buffer {
        let size = (capacity * VISIBLE_INDEX_SIZE) as wgpu::BufferAddress;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("note_visible_index_buffer"),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu_resource_tracker::add_buffer(&buffer);
        buffer
    }

    /// 扩容可见索引缓冲区
    ///
    /// 用户硬约束：不得限制 GPU 内存使用——移除 .min(self.max_capacity) 封顶，
    /// 实际容量仅受 wgpu 硬件限制。
    pub(super) fn grow_visible_buffer(&mut self, device: &wgpu::Device, required_capacity: usize) {
        puffin::profile_function!();
        let growth_factor = crate::constants::rendering::BUFFER_GROWTH_FACTOR;
        let mut new_capacity = (self.capacity.saturating_mul(growth_factor)).max(required_capacity);

        if new_capacity <= self.capacity {
            return;
        }

        // 限制容量余量：超大可见 buffer 不因单个增量而翻倍。
        const MAX_EXTRA_INSTANCES: usize = 1_048_576;
        let extra = new_capacity.saturating_sub(required_capacity);
        if extra > MAX_EXTRA_INSTANCES {
            new_capacity = required_capacity.saturating_add(MAX_EXTRA_INSTANCES);
        }

        tracing::debug!(
            "Growing note visible index buffer: {} -> {} (required: {})",
            self.capacity,
            new_capacity,
            required_capacity
        );

        gpu_resource_tracker::sub_buffer(&self.visible_instance_buffer);
        self.visible_instance_buffer = Self::create_visible_index_buffer(device, new_capacity);
        self.capacity = new_capacity;
    }

    /// 可见索引缓冲区布局描述（渲染管线顶点输入）
    pub(super) fn visible_index_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: VISIBLE_INDEX_SIZE as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &VISIBLE_INDEX_ATTRIBUTES,
        }
    }

    /// 创建渲染 bind groups（每 chunk 一个，规避 storage binding 2GB 上限）
    ///
    /// 2026-08-07：顶点着色器从 all_instances storage buffer 读取原实例数据，
    /// 因此渲染 bind group 也需要按 chunk 绑定 source buffer 切片，
    /// 避免大 buffer 整体绑定超过 `max_storage_buffer_binding_size`。
    pub(super) fn create_render_bind_groups(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        view_state_buffer: &wgpu::Buffer,
        instance_buffer: &wgpu::Buffer,
        visible_instance_buffer: &wgpu::Buffer,
        chunk_layout: &ChunkLayout,
    ) -> Vec<wgpu::BindGroup> {
        puffin::profile_function!();
        let instance_size = std::mem::size_of::<NoteInstance>() as u64;
        let visible_capacity_instances =
            (visible_instance_buffer.size() / VISIBLE_INDEX_SIZE as u64) as usize;
        let source_capacity_instances = (instance_buffer.size() / instance_size) as usize;
        let capacity_instances = source_capacity_instances.min(visible_capacity_instances);
        let chunk_count = chunk_layout.chunk_count(capacity_instances).min(super::chunk::MAX_CHUNKS);

        let mut bind_groups = Vec::with_capacity(chunk_count);
        for idx in 0..chunk_count {
            let (chunk_start, chunk_len) = chunk_layout.chunk_range(capacity_instances, idx);
            let chunk_bytes = (chunk_len as u64) * instance_size;
            let chunk_offset = (chunk_start as u64) * instance_size;

            bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: instance_buffer,
                            offset: chunk_offset,
                            size: std::num::NonZeroU64::new(chunk_bytes),
                        }),
                    },
                ],
            }));
        }
        bind_groups
    }

    /// 创建计算 bind groups（按 chunk 切片，规避 storage binding 2GB 上限）
    ///
    /// 每 chunk 一个 bind group，各绑定自己的 offset/size 切片：
    /// - binding 0/1: viewport / cull_uniform（uniform 槽位 offset = idx × slot_align）
    /// - binding 2:   实例 buffer 切片 [chunk_start × 16, chunk 字节范围)
    /// - binding 3:   可见索引 buffer 切片 [chunk_start × 4, chunk_len × 4)
    /// - binding 4:   indirect buffer 槽位（idx × slot_align，32B DrawIndirectArgs）
    ///
    /// 分区基准 = 实例 buffer 与可见索引 buffer 容量的**较小值**（按实例数对齐，
    /// 可见 buffer 字节数 = 容量 × 4）。两者独立扩容，分区必须同步，
    /// 否则 first_instance = chunk_start 语义失效。
    /// shader 以 uniform 的 chunk_count 限制访问范围，未初始化的尾部数据不会被读取。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_cull_bind_groups(
        device: &wgpu::Device,
        layout: &ChunkLayout,
        instance_buffer: &wgpu::Buffer,
        visible_instance_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
        cull_uniform_buffer: &wgpu::Buffer,
        viewport_buffer: &wgpu::Buffer,
        cull_uniform_buffer_size: u64,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Vec<wgpu::BindGroup> {
        puffin::profile_function!();
        let instance_size = std::mem::size_of::<NoteInstance>() as u64;

        // 可见索引 buffer 按 u32 分区，容量（实例数）= 字节数 / 4
        let visible_capacity_instances = (visible_instance_buffer.size() / VISIBLE_INDEX_SIZE as u64) as usize;
        let source_capacity_instances = (instance_buffer.size() / instance_size) as usize;
        let capacity_instances = source_capacity_instances.min(visible_capacity_instances);
        let chunk_count = layout
            .chunk_count(capacity_instances)
            .min(super::chunk::MAX_CHUNKS);

        let mut bind_groups = Vec::with_capacity(chunk_count);
        for idx in 0..chunk_count {
            let (chunk_start, chunk_len) = layout.chunk_range(capacity_instances, idx);
            let chunk_bytes = (chunk_len as u64) * instance_size;
            let chunk_offset = (chunk_start as u64) * instance_size;
            // 可见索引 buffer 的切片：按 u32 偏移/长度
            let visible_chunk_bytes = (chunk_len as u64) * VISIBLE_INDEX_SIZE as u64;
            let visible_chunk_offset = (chunk_start as u64) * VISIBLE_INDEX_SIZE as u64;
            let slot_offset = layout.chunk_offset_bytes(idx);

            // 每 chunk 的 uniform 条目（chunk_start/chunk_count 在 dispatch 前写入）
            let uniform_size = std::num::NonZeroU64::new(16).expect("CullUniform 16 bytes");
            debug_assert!(slot_offset + 16 <= cull_uniform_buffer_size);

            let instance_binding = wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: instance_buffer,
                offset: chunk_offset,
                size: std::num::NonZeroU64::new(chunk_bytes),
            });
            let visible_binding = wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: visible_instance_buffer,
                offset: visible_chunk_offset,
                size: std::num::NonZeroU64::new(visible_chunk_bytes),
            });

            bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("note_cull_bind_group"),
                layout: bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: viewport_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: cull_uniform_buffer,
                            offset: slot_offset,
                            size: Some(uniform_size),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: instance_binding,
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: visible_binding,
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: indirect_buffer,
                            offset: slot_offset,
                            size: std::num::NonZeroU64::new(std::mem::size_of::<
                                super::types::DrawIndirectArgs,
                            >() as u64),
                        }),
                    },
                ],
            }));
        }
        bind_groups
    }
}
