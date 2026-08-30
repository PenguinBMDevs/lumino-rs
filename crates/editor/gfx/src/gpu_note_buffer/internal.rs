//! GPU 音符缓冲区内部方法

use super::types::GpuNoteBuffer;
use crate::gpu_resource_tracker::TrackedBuffer;

impl GpuNoteBuffer {
    /// 创建缓冲区
    pub(crate) fn create_buffer(device: &wgpu::Device, capacity: usize) -> TrackedBuffer {
        let size = (capacity * std::mem::size_of::<crate::NoteInstance>()) as wgpu::BufferAddress;

        TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("gpu_note_buffer"),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC
                    // 走带视图直接复用该常驻缓冲作为顶点缓冲（零第二份显存）
                    | wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            },
        )
    }

    /// 扩容缓冲区
    ///
    /// 用户硬约束：不得限制 GPU 内存使用——移除 .min(self.max_capacity) 封顶，
    /// 实际容量仅受 wgpu 硬件限制（max_storage_buffer_binding_size）。
    /// 若超出硬件限制，create_buffer 会返回错误。
    ///
    /// 旧缓冲在替换引用时由 [`TrackedBuffer`] Drop 自动注销，无需手动 `sub_buffer`。
    pub(crate) fn grow(&mut self, required_capacity: usize) -> bool {
        puffin::profile_function!();
        let mut new_capacity = self
            .capacity
            .saturating_mul(Self::GROWTH_FACTOR)
            .max(required_capacity);

        if new_capacity <= self.capacity {
            return false;
        }

        // 限制容量余量：超大 buffer 不因单个增量而翻倍，避免 4.6GB → 9.2GB 浪费。
        const MAX_EXTRA_INSTANCES: usize = 1_048_576;
        let extra = new_capacity.saturating_sub(required_capacity);
        if extra > MAX_EXTRA_INSTANCES {
            new_capacity = required_capacity.saturating_add(MAX_EXTRA_INSTANCES);
        }

        tracing::info!(
            "GpuNoteBuffer: growing {} -> {} (required: {})",
            self.capacity,
            new_capacity,
            required_capacity
        );

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
                encoder.copy_buffer_to_buffer(
                    self.instance_buffer.inner(),
                    0,
                    new_buffer.inner(),
                    0,
                    copy_size,
                );
            }

            // 提交命令
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        self.instance_buffer = new_buffer;
        self.capacity = new_capacity;

        true
    }
}
