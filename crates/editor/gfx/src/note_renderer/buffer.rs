use super::types::{NoteInstance, VERTEX_ATTRIBUTES};
use crate::{gpu_resource_tracker, note_renderer::NoteRenderer};

/// Cull bind group 的 GPU 缓冲区集合
pub(super) struct CullBuffers<'a> {
    pub layout: &'a wgpu::BindGroupLayout,
    pub viewport_buffer: &'a wgpu::Buffer,
    pub cull_uniform_buffer: &'a wgpu::Buffer,
    pub instance_buffer: &'a wgpu::Buffer,
    pub visible_instance_buffer: &'a wgpu::Buffer,
    pub indirect_buffer: &'a wgpu::Buffer,
    pub instance_count: usize,
}

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

        // 重新创建 cull bind group（扩容后使用当前上传的实例数）
        let cull_buffers = CullBuffers {
            layout: &self.cull_bind_group_layout,
            viewport_buffer: &self.viewport_buffer,
            cull_uniform_buffer: &self.cull_uniform_buffer,
            instance_buffer: self.gpu_note_buffer.buffer(),
            visible_instance_buffer: &self.visible_instance_buffer,
            indirect_buffer: &self.indirect_buffer,
            instance_count: self.last_upload_count as usize,
        };
        self.cull_bind_group = Self::create_cull_bind_group(device, &cull_buffers);
    }

    /// 实例缓冲区布局描述
    pub(super) fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<NoteInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &VERTEX_ATTRIBUTES,
        }
    }

    /// 创建计算 bind group（带实际数据大小限制）
    pub(super) fn create_cull_bind_group(
        device: &wgpu::Device,
        buffers: &CullBuffers<'_>,
    ) -> wgpu::BindGroup {
        puffin::profile_function!();
        let instance_size = std::mem::size_of::<NoteInstance>() as u64;
        let actual_data_size = (buffers.instance_count as u64) * instance_size;
        let buffer_size = buffers.instance_buffer.size();

        // 限制绑定范围到实际数据大小，避免GPU预取超出范围
        let instance_binding = if let Some(size) = std::num::NonZeroU64::new(actual_data_size) {
            if actual_data_size < buffer_size {
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: buffers.instance_buffer,
                    offset: 0,
                    size: Some(size),
                })
            } else {
                buffers.instance_buffer.as_entire_binding()
            }
        } else {
            buffers.instance_buffer.as_entire_binding()
        };

        let visible_binding = if let Some(size) = std::num::NonZeroU64::new(actual_data_size) {
            if actual_data_size < buffers.visible_instance_buffer.size() {
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: buffers.visible_instance_buffer,
                    offset: 0,
                    size: Some(size),
                })
            } else {
                buffers.visible_instance_buffer.as_entire_binding()
            }
        } else {
            buffers.visible_instance_buffer.as_entire_binding()
        };

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("note_cull_bind_group"),
            layout: buffers.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers.cull_uniform_buffer.as_entire_binding(),
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
                    resource: buffers.indirect_buffer.as_entire_binding(),
                },
            ],
        })
    }
}
