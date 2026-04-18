use super::types::{NoteInstance, VERTEX_ATTRIBUTES};
use crate::note_renderer::NoteRenderer;

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

        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(if is_storage {
                "note_visible_instance_buffer"
            } else {
                "note_instance_buffer"
            }),
            size: (capacity * std::mem::size_of::<NoteInstance>()) as wgpu::BufferAddress,
            usage,
            mapped_at_creation: false,
        })
    }

    /// 扩容缓冲区（受 max_capacity 限制）
    pub(super) fn grow_visible_buffer(&mut self, device: &wgpu::Device, required_capacity: usize) {
        puffin::profile_function!();
        let growth_factor = crate::constants::rendering::BUFFER_GROWTH_FACTOR;
        let new_capacity = ((self.capacity.saturating_mul(growth_factor)).max(required_capacity))
            .min(self.max_capacity);
        if new_capacity <= self.capacity {
            return;
        }

        tracing::debug!(
            "Growing note buffer: {} -> {} (required: {})",
            self.capacity,
            new_capacity,
            required_capacity
        );

        self.visible_instance_buffer = Self::create_instance_buffer(device, new_capacity, true);
        self.capacity = new_capacity;

        // 重新创建 cull bind group（扩容后使用当前上传的实例数）
        self.cull_bind_group = Self::create_cull_bind_group(
            device,
            &self.cull_bind_group_layout,
            &self.viewport_buffer,
            &self.cull_uniform_buffer,
            self.gpu_note_buffer.buffer(),
            &self.visible_instance_buffer,
            &self.indirect_buffer,
            self.last_upload_count as usize,
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

    /// 创建计算 bind group（带实际数据大小限制）
    pub(super) fn create_cull_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        cull_uniform_buffer: &wgpu::Buffer,
        instance_buffer: &wgpu::Buffer,
        visible_instance_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
        instance_count: usize,
    ) -> wgpu::BindGroup {
        puffin::profile_function!();
        let instance_size = std::mem::size_of::<NoteInstance>() as u64;
        let actual_data_size = (instance_count as u64) * instance_size;
        let buffer_size = instance_buffer.size();

        // 限制绑定范围到实际数据大小，避免GPU预取超出范围
        let instance_binding = if let Some(size) = std::num::NonZeroU64::new(actual_data_size) {
            if actual_data_size < buffer_size {
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: instance_buffer,
                    offset: 0,
                    size: Some(size),
                })
            } else {
                instance_buffer.as_entire_binding()
            }
        } else {
            instance_buffer.as_entire_binding()
        };

        let visible_binding = if let Some(size) = std::num::NonZeroU64::new(actual_data_size) {
            if actual_data_size < visible_instance_buffer.size() {
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: visible_instance_buffer,
                    offset: 0,
                    size: Some(size),
                })
            } else {
                visible_instance_buffer.as_entire_binding()
            }
        } else {
            visible_instance_buffer.as_entire_binding()
        };

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("note_cull_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cull_uniform_buffer.as_entire_binding(),
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
                    resource: indirect_buffer.as_entire_binding(),
                },
            ],
        })
    }
}
