//! GPU 音符缓冲区内部方法

use super::types::GpuNoteBuffer;
use crate::gpu_resource_tracker;

impl GpuNoteBuffer {
    /// 创建缓冲区
    pub(crate) fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let size = (capacity * std::mem::size_of::<crate::NoteInstance>()) as wgpu::BufferAddress;

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_note_buffer"),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        gpu_resource_tracker::add_buffer(&buffer);
        buffer
    }

    /// 扩容缓冲区
    ///
    /// 用户硬约束：不得限制 GPU 内存使用——移除 .min(self.max_capacity) 封顶，
    /// 实际容量仅受 wgpu 硬件限制（max_storage_buffer_binding_size）。
    /// 若超出硬件限制，create_buffer 会返回错误。
    pub(crate) fn grow(&mut self, required_capacity: usize) -> bool {
        puffin::profile_function!();
        let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(required_capacity);

        if new_capacity <= self.capacity {
            return false;
        }

        tracing::info!(
            "GpuNoteBuffer: growing {} -> {} (required: {})",
            self.capacity,
            new_capacity,
            required_capacity
        );

        // 释放旧缓冲区内存计数（在创建新缓冲区前扣除，避免瞬间重复计数）
        gpu_resource_tracker::sub_buffer(&self.instance_buffer);

        // 创建新缓冲区
        let new_buffer = Self::create_buffer(&self.device, new_capacity);

        // 如果有现有数据，需要复制到新缓冲区
        if self.instance_count > 0 {
            // 创建命令编码器
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu_note_buffer_grow"),
                });

            // 复制旧数据到新缓冲区
            let copy_size =
                (self.instance_count * std::mem::size_of::<crate::NoteInstance>()) as u64;
            {
                puffin::profile_scope!("grow_buffer_copy");
                encoder.copy_buffer_to_buffer(&self.instance_buffer, 0, &new_buffer, 0, copy_size);
            }

            // 提交命令
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        self.instance_buffer = new_buffer;
        self.capacity = new_capacity;

        true
    }
}
