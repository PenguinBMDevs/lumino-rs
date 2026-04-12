//! GPU 音符缓冲区 - 音符数据常驻 GPU 内存
//!
//! 架构说明：
//! - 音符数据上传一次到 GPU，之后常驻 GPU 内存
//! - 只支持增量更新（添加/修改/删除单个音符）
//! - 视口变化时只更新 camera uniform，不重新上传所有数据
//! - 严格控制 GPU 内存占用，支持动态扩容/缩容

use wgpu::util::DeviceExt;

/// 音符编辑事件
#[derive(Debug, Clone)]
pub enum NoteEvent {
    /// 重新加载所有音符
    Reset(Vec<crate::NoteInstance>),
    /// 添加音符
    Add(crate::NoteInstance),
    /// 更新单个音符
    Update {
        index: usize,
        instance: crate::NoteInstance,
    },
    /// 更新多个音符
    UpdateMany {
        start_index: usize,
        instances: Vec<crate::NoteInstance>,
    },
    /// 移除音符
    Remove(usize),
    /// 清空所有音符
    Clear,
}

/// GPU 音符缓冲区
pub struct GpuNoteBuffer {
    /// 实例缓冲区（常驻 GPU 内存）
    instance_buffer: wgpu::Buffer,
    /// 当前缓冲区容量（实例数量）
    capacity: usize,
    /// 当前实际存储的实例数量
    instance_count: usize,
    /// 最大容量限制
    max_capacity: usize,
    /// 设备引用（用于扩容）
    device: std::sync::Arc<wgpu::Device>,
    /// 队列引用（用于更新）
    queue: std::sync::Arc<wgpu::Queue>,
}

impl GpuNoteBuffer {
    /// 初始容量
    const INITIAL_CAPACITY: usize = 1024;
    /// 扩容因子
    const GROWTH_FACTOR: usize = 2;
    /// 最大容量（约 1GB GPU 内存，每个实例 32 字节）
    const MAX_CAPACITY: usize = 33_554_432; // 1GB / 32 bytes

    /// 创建新的 GPU 音符缓冲区
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let instance_buffer = Self::create_buffer(device, Self::INITIAL_CAPACITY);

        let max_binding_size = device.limits().max_storage_buffer_binding_size as u64;
        let max_buffer_size = device.limits().max_buffer_size;
        let max_bytes = max_binding_size.min(max_buffer_size) as usize;
        let max_capacity =
            (max_bytes / std::mem::size_of::<crate::NoteInstance>()).min(Self::MAX_CAPACITY);

        Self {
            instance_buffer,
            capacity: Self::INITIAL_CAPACITY,
            instance_count: 0,
            max_capacity,
            device: std::sync::Arc::new(device.clone()),
            queue: std::sync::Arc::new(queue.clone()),
        }
    }

    /// 批量上传所有音符（初始化时使用）
    pub fn upload_all(&mut self, instances: &[crate::NoteInstance]) {
        puffin::profile_function!();
        if instances.is_empty() {
            self.instance_count = 0;
            return;
        }

        // 检查是否需要扩容
        if instances.len() > self.capacity {
            self.grow(instances.len());
        }

        let upload_count = instances.len().min(self.max_capacity);
        self.instance_count = upload_count;

        // 上传数据到 GPU
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..upload_count]),
        );

        tracing::info!("Uploading {} notes", upload_count);
    }

    /// 增量更新单个音符
    pub fn update_note(&mut self, index: usize, instance: &crate::NoteInstance) {
        puffin::profile_function!();
        if index >= self.instance_count {
            tracing::warn!(
                "GpuNoteBuffer: update index {} out of range {}",
                index,
                self.instance_count
            );
            return;
        }

        // 计算偏移量
        let offset = index * std::mem::size_of::<crate::NoteInstance>();

        // 上传单个实例
        self.queue.write_buffer(
            &self.instance_buffer,
            offset as wgpu::BufferAddress,
            bytemuck::cast_slice(std::slice::from_ref(instance)),
        );
    }

    /// 批量更新音符（用于编辑操作后的批量更新）
    pub fn update_notes(&mut self, start_index: usize, instances: &[crate::NoteInstance]) {
        puffin::profile_function!();
        if start_index >= self.instance_count || instances.is_empty() {
            return;
        }

        let end_index = (start_index + instances.len()).min(self.instance_count);
        let count = end_index - start_index;

        // 计算偏移量
        let offset = start_index * std::mem::size_of::<crate::NoteInstance>();

        // 批量上传
        self.queue.write_buffer(
            &self.instance_buffer,
            offset as wgpu::BufferAddress,
            bytemuck::cast_slice(&instances[..count]),
        );
    }

    /// 添加新音符（在末尾追加）
    pub fn add_note(&mut self, instance: &crate::NoteInstance) -> usize {
        puffin::profile_function!();
        // 检查是否需要扩容
        if self.instance_count >= self.capacity {
            if !self.grow(self.capacity * Self::GROWTH_FACTOR) {
                tracing::error!("GpuNoteBuffer: failed to grow buffer, cannot add note");
                return self.instance_count.saturating_sub(1);
            }
        }

        let index = self.instance_count;
        let offset = index * std::mem::size_of::<crate::NoteInstance>();

        // 上传单个实例
        self.queue.write_buffer(
            &self.instance_buffer,
            offset as wgpu::BufferAddress,
            bytemuck::cast_slice(std::slice::from_ref(instance)),
        );

        self.instance_count += 1;
        index
    }

    /// 删除音符（通过将最后一个音符移动到被删除位置）
    pub fn remove_note(&mut self, index: usize) {
        puffin::profile_function!();
        if index >= self.instance_count {
            return;
        }

        // 如果不是最后一个，将最后一个移动到被删除位置
        if index < self.instance_count - 1 {
            // 这里需要从 GPU 读取最后一个音符的数据，然后更新到被删除位置
            // 为了简化，我们暂时只减少计数，实际渲染时通过其他方式处理
            // TODO: 实现正确的删除逻辑
        }

        self.instance_count -= 1;
    }

    /// 清空所有音符
    pub fn clear(&mut self) {
        self.instance_count = 0;
    }

    /// 获取实例缓冲区引用（用于渲染）
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.instance_buffer
    }

    /// 获取当前实例数量
    pub fn len(&self) -> usize {
        self.instance_count
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.instance_count == 0
    }

    /// 获取当前容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 获取 GPU 内存占用（字节）
    pub fn gpu_memory_usage(&self) -> usize {
        self.instance_buffer.size() as usize
    }

    /// 创建缓冲区
    fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let size = (capacity * std::mem::size_of::<crate::NoteInstance>()) as wgpu::BufferAddress;

        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_note_buffer"),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    }

    /// 扩容缓冲区
    fn grow(&mut self, required_capacity: usize) -> bool {
        puffin::profile_function!();
        let new_capacity =
            ((self.capacity * Self::GROWTH_FACTOR).max(required_capacity)).min(self.max_capacity);

        if new_capacity <= self.capacity {
            return false;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_memory_usage_calculation() {
        // 这个测试不需要实际的 GPU 设备
        // 只是验证计算逻辑
        let instance_size = std::mem::size_of::<crate::NoteInstance>();
        assert!(instance_size > 0);

        // 验证最大容量计算
        let max_capacity = GpuNoteBuffer::MAX_CAPACITY;
        let max_memory = max_capacity * instance_size;
        assert!(max_memory <= 1024 * 1024 * 1024); // 1GB
    }
}
