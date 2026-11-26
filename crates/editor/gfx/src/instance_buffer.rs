//! 实例缓冲区：容量、扩容与上传的统一实现。
//!
//! 三个渲染器此前各写一份「容量判断 → 重建 TrackedBuffer → write_buffer」。
//! 这里统一扩容策略（`capacity * factor`，可选额外冗余上限），并允许自定义 usage
//! （note 的可见索引是 `STORAGE | VERTEX`，其余是 `VERTEX`）。

use std::marker::PhantomData;

use bytemuck::Pod;

use crate::gpu_resource_tracker::TrackedBuffer;

/// 实例 / 索引缓冲区。
pub(crate) struct InstanceBuffer<T: Pod> {
    buffer: TrackedBuffer,
    capacity: usize,
    label: &'static str,
    usage: wgpu::BufferUsages,
    _marker: PhantomData<T>,
}

impl<T: Pod> InstanceBuffer<T> {
    /// 创建缓冲区。
    pub(crate) fn new(
        device: &wgpu::Device,
        label: &'static str,
        capacity: usize,
        usage: wgpu::BufferUsages,
    ) -> Self {
        Self {
            buffer: Self::allocate(device, label, capacity, usage),
            capacity,
            label,
            usage,
            _marker: PhantomData,
        }
    }

    fn allocate(
        device: &wgpu::Device,
        label: &'static str,
        capacity: usize,
        usage: wgpu::BufferUsages,
    ) -> TrackedBuffer {
        TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some(label),
                size: (capacity.max(1) * std::mem::size_of::<T>()) as wgpu::BufferAddress,
                usage,
                mapped_at_creation: false,
            },
        )
    }

    /// 当前容量（元素个数）。
    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    /// 底层 buffer（绑定 / 诊断用）。
    pub(crate) fn buffer(&self) -> &wgpu::Buffer {
        self.buffer.inner()
    }

    /// 确保容量不小于 `required`；返回是否重建了 buffer。
    ///
    /// 策略见 [`next_capacity`]：先按 `capacity * growth_factor` 扩，再 `max(required)`。
    pub(crate) fn ensure_capacity(
        &mut self,
        device: &wgpu::Device,
        required: usize,
        growth_factor: usize,
    ) -> bool {
        let Some(new_capacity) = next_capacity(self.capacity, required, growth_factor, 0) else {
            return false;
        };
        self.buffer = Self::allocate(device, self.label, new_capacity, self.usage);
        self.capacity = new_capacity;
        true
    }

    /// 上传实例数据（容量由调用方保证）。
    pub(crate) fn write(&self, queue: &wgpu::Queue, instances: &[T]) {
        if instances.is_empty() {
            return;
        }
        queue.write_buffer(self.buffer.inner(), 0, bytemuck::cast_slice(instances));
    }
}

/// 扩容策略：`capacity * factor`，至少 `required`；`extra_cap > 0` 时限制额外冗余。
///
/// 返回 `None` 表示无需扩容。
pub(crate) fn next_capacity(
    current: usize,
    required: usize,
    growth_factor: usize,
    extra_cap: usize,
) -> Option<usize> {
    if required <= current {
        return None;
    }
    let mut new_capacity = current.saturating_mul(growth_factor).max(required);
    if extra_cap > 0 {
        let extra = new_capacity.saturating_sub(required);
        if extra > extra_cap {
            new_capacity = required.saturating_add(extra_cap);
        }
    }
    (new_capacity > current).then_some(new_capacity)
}

#[cfg(test)]
mod tests {
    use super::next_capacity;

    #[test]
    fn grows_by_factor_when_required_is_small() {
        assert_eq!(next_capacity(100, 120, 2, 0), Some(200));
    }

    #[test]
    fn grows_to_required_when_needed() {
        assert_eq!(next_capacity(100, 300, 2, 0), Some(300));
    }

    #[test]
    fn no_growth_when_capacity_suffices() {
        assert_eq!(next_capacity(100, 50, 2, 0), None);
        assert_eq!(next_capacity(100, 100, 2, 0), None);
    }

    #[test]
    fn extra_cap_limits_slack() {
        // 100 * 2 = 200，required = 120，slack 80 > 32 → 120 + 32
        assert_eq!(next_capacity(100, 120, 2, 32), Some(152));
    }
}
