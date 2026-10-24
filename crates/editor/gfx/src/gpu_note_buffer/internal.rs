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

    /// 计算扩容后的目标容量（纯函数，单测覆盖边界）。
    ///
    /// 关键约束：**余量不能把容量推过 `max_capacity`**。旧实现先算
    /// `required + 1,048,576` 余量、再直接建 buffer，required 接近上限时
    /// 会创建出 2.16GB 的 buffer——超过 `max_storage_buffer_binding_size`
    /// （2GB-1），瀑布流 `as_entire_buffer_binding()` 立刻 validation error 刷屏。
    fn next_capacity(
        current: usize,
        required: usize,
        max: usize,
        growth_factor: usize,
    ) -> Option<usize> {
        if required > max {
            return None;
        }
        let mut new_capacity = current.saturating_mul(growth_factor).max(required);
        if new_capacity <= current {
            return None;
        }

        // 限制容量余量：超大 buffer 不因单个增量而翻倍。
        const MAX_EXTRA_INSTANCES: usize = 1_048_576;
        let extra = new_capacity.saturating_sub(required);
        if extra > MAX_EXTRA_INSTANCES {
            new_capacity = required.saturating_add(MAX_EXTRA_INSTANCES);
        }

        // 硬上限：宁可少留余量，也绝不创建超过 binding 上限的 buffer。
        new_capacity = new_capacity.min(max);
        (new_capacity >= required).then_some(new_capacity)
    }

    /// 扩容缓冲区
    ///
    /// 容量受 `max_capacity`（`max_storage_buffer_binding_size / 16`）硬限制。
    /// 超出时明确返回 `false`，调用方不得再向旧 buffer 越界写。
    ///
    /// 旧缓冲在替换引用时由 [`TrackedBuffer`] Drop 自动注销，无需手动 `sub_buffer`。
    pub(crate) fn grow(&mut self, required_capacity: usize) -> bool {
        puffin::profile_function!();
        let Some(new_capacity) = Self::next_capacity(
            self.capacity,
            required_capacity,
            self.max_capacity,
            Self::GROWTH_FACTOR,
        ) else {
            // 超限后每个 chunk 都会再尝试一次；只在首个失败时 error，避免刷屏。
            if self.dropped_instances == 0 {
                tracing::error!(
                    "GpuNoteBuffer: 需要 {} 个实例，超过硬件上限 max_capacity {}（或容量无法增长），拒绝扩容",
                    required_capacity,
                    self.max_capacity
                );
            } else {
                tracing::debug!(
                    "GpuNoteBuffer: 继续拒绝扩容（需要 {}，上限 {}）",
                    required_capacity,
                    self.max_capacity
                );
            }
            return false;
        };

        tracing::info!(
            "GpuNoteBuffer: growing {} -> {} (required: {}, max: {})",
            self.capacity,
            new_capacity,
            required_capacity,
            self.max_capacity
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

#[cfg(test)]
mod growth_tests {
    use super::GpuNoteBuffer;

    /// 4GB / 12B 的整数容量（12B 源缓冲只受 max_buffer_size 约束）
    const MAX: usize = 357_913_941;

    #[test]
    fn test_growth_margin_never_exceeds_binding_capacity() {
        // 触发真机 bug 的形态：required 接近上限，扩容余量不得越界
        let required = 356_913_941;
        let next = GpuNoteBuffer::next_capacity(178_956_928, required, MAX, 2)
            .expect("required 未超上限，必须能扩容");
        assert!(
            next <= MAX,
            "扩容余量不得把容量推过 max_buffer 上限：{next} > {MAX}"
        );
        assert!(next >= required);
        assert!(
            next <= MAX && next >= required,
            "接近上限时必须落在 [required, max_capacity]"
        );
    }

    #[test]
    fn test_growth_at_exact_limit_allowed() {
        let next = GpuNoteBuffer::next_capacity(100_000_000, MAX, MAX, 2)
            .expect("required == max 必须允许");
        assert_eq!(next, MAX);
    }

    #[test]
    fn test_growth_over_limit_rejected() {
        assert!(
            GpuNoteBuffer::next_capacity(100_000_000, MAX + 1, MAX, 2).is_none(),
            "超过 max_capacity 必须拒绝，而不是创建超限 buffer"
        );
    }

    #[test]
    fn test_growth_normal_doubling_kept() {
        let next = GpuNoteBuffer::next_capacity(1_000_000, 1_500_000, MAX, 2).expect("常规扩容");
        assert_eq!(next, 2_000_000, "常规场景保留翻倍余量");
    }

    #[test]
    fn test_growth_extra_capped() {
        let next = GpuNoteBuffer::next_capacity(10_000_000, 10_500_000, MAX, 2)
            .expect("超大 buffer 不翻倍");
        assert_eq!(next, 10_500_000 + 1_048_576, "余量上限 1,048,576");
    }

    #[test]
    fn test_growth_noop_rejected() {
        assert!(
            GpuNoteBuffer::next_capacity(10_000_000, 5_000_000, MAX, 1).is_none(),
            "不需要增长时不得返回同一个容量"
        );
    }
}
