use super::chunk::ChunkLayout;
use super::types::{NoteInstance, VERTEX_ATTRIBUTES};
use crate::{gpu_resource_tracker, note_renderer::NoteRenderer};

impl NoteRenderer {
    /// 创建实例缓冲区
    pub(super) fn create_instance_buffer(
        device: &wgpu::Device,
        capacity: usize,
        is_storage: bool,
    ) -> wgpu::Buffer {
        let mut usage = wgpu::BufferUsages::COPY_DST;
        if is_storage {
            usage |= wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX;
        } else {
            usage |= wgpu::BufferUsages::STORAGE;
        }

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(if is_storage {
                "note_visible_instance_buffer"
            } else {
                "note_instance_buffer"
            }),
            size: (capacity * std::mem::size_of::<NoteInstance>()) as wgpu::BufferAddress,
            usage,
            mapped_at_creation: false,
        });
        gpu_resource_tracker::add_buffer(&buffer);
        buffer
    }

    /// 扩容缓冲区
    ///
    /// 用户硬约束：不得限制 GPU 内存使用——移除 .min(self.max_capacity) 封顶，
    /// 实际容量仅受 wgpu 硬件限制。
    pub(super) fn grow_visible_buffer(&mut self, device: &wgpu::Device, required_capacity: usize) {
        puffin::profile_function!();
        let growth_factor = crate::constants::rendering::BUFFER_GROWTH_FACTOR;
        let new_capacity = (self.capacity.saturating_mul(growth_factor)).max(required_capacity);
        if new_capacity <= self.capacity {
            return;
        }

        tracing::debug!(
            "Growing note buffer: {} -> {} (required: {})",
            self.capacity,
            new_capacity,
            required_capacity
        );

        gpu_resource_tracker::sub_buffer(&self.visible_instance_buffer);
        self.visible_instance_buffer = Self::create_instance_buffer(device, new_capacity, true);
        self.capacity = new_capacity;

        // 重新创建 cull bind groups（扩容后按当前数据量分块）
        self.cull_bind_groups = Self::create_cull_bind_groups(
            device,
            &self.chunk_layout,
            self.gpu_note_buffer.buffer(),
            &self.visible_instance_buffer,
            &self.indirect_buffer,
            &self.cull_uniform_buffer,
            &self.viewport_buffer,
            self.cull_uniform_buffer_size,
            &self.cull_bind_group_layout,
        );
    }

    /// 实例缓冲区布局描述
    pub(super) fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<NoteInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &VERTEX_ATTRIBUTES,
        }
    }

    /// 创建计算 bind groups（按 chunk 切片，规避 storage binding 2GB 上限）
    ///
    /// 每 chunk 一个 bind group，各绑定自己的 offset/size 切片：
    /// - binding 0/1: viewport / cull_uniform（uniform 槽位 offset = idx × slot_align）
    /// - binding 2:   实例 buffer 切片 [chunk_start × 16, chunk 字节范围)
    /// - binding 3:   可见 buffer 切片（与实例区同分区，first_instance = chunk_start）
    /// - binding 4:   indirect buffer 槽位（idx × slot_align，32B DrawIndirectArgs）
    ///
    /// 分区基准 = 实例 buffer 与可见 buffer 容量的**较小值**（两者独立扩容，
    /// 分区必须同步，否则 first_instance = chunk_start 语义失效）。
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

        let capacity_instances = (instance_buffer.size() / instance_size)
            .min(visible_instance_buffer.size() / instance_size)
            as usize;
        let chunk_count = layout
            .chunk_count(capacity_instances)
            .min(super::chunk::MAX_CHUNKS);

        let mut bind_groups = Vec::with_capacity(chunk_count);
        for idx in 0..chunk_count {
            let (chunk_start, chunk_len) = layout.chunk_range(capacity_instances, idx);
            let chunk_bytes = (chunk_len as u64) * instance_size;
            let chunk_offset = (chunk_start as u64) * instance_size;
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
                offset: chunk_offset,
                size: std::num::NonZeroU64::new(chunk_bytes),
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
