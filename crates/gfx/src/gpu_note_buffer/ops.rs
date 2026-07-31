//! GPU 音符缓冲区公开操作

use super::types::GpuNoteBuffer;

impl GpuNoteBuffer {
    /// 初始容量
    pub const INITIAL_CAPACITY: usize = 1024;

    /// 扩容因子
    pub const GROWTH_FACTOR: usize = 2;

    /// 最大容量（约 1GB GPU 内存，每个实例 32 字节）
    pub const MAX_CAPACITY: usize = 33_554_432; // 1GB / 32 bytes

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
            instances: Vec::with_capacity(Self::INITIAL_CAPACITY),
        }
    }

    /// 批量上传所有音符（初始化时使用）
    ///
    /// 优化：
    /// 1. 使用 extend_from_slice 替代 to_vec，避免重复分配
    /// 2. 预分配 capacity，减少重新分配
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
        if instances.len() > self.max_capacity {
            tracing::warn!(
                "GpuNoteBuffer: instance count {} exceeds max_capacity {}, truncated to {}",
                instances.len(),
                self.max_capacity,
                upload_count
            );
        }
        self.instance_count = upload_count;

        // 优化：复制到 CPU 缓存后直接从缓存上传 GPU
        // 避免对输入 slice 的二次遍历（write_buffer 也会 memcpy）
        //
        // 数据流：instances → self.instances（一次遍历）→ write_buffer（缓存热点）
        // 原方案：instances → self.instances（一次遍历）+ instances → write_buffer（二次遍历）
        self.instances.clear();
        self.instances.extend_from_slice(&instances[..upload_count]);

        // 从 CPU 缓存上传 GPU — 此时 self.instances 的数据仍在 L1/L2 cache 中
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&self.instances),
        );

        tracing::debug!("Uploading {} notes", upload_count);
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

        // 更新 CPU 缓存
        self.instances[index] = *instance;

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

        // 更新 CPU 缓存
        for (index, instance) in instances[..count].iter().enumerate() {
            self.instances[start_index + index] = *instance;
        }

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
        if self.instance_count >= self.capacity && !self.grow(self.capacity * Self::GROWTH_FACTOR) {
            tracing::error!("GpuNoteBuffer: failed to grow buffer, cannot add note");
            return self.instance_count.saturating_sub(1);
        }

        let index = self.instance_count;
        let offset = index * std::mem::size_of::<crate::NoteInstance>();

        // 上传单个实例
        self.queue.write_buffer(
            &self.instance_buffer,
            offset as wgpu::BufferAddress,
            bytemuck::cast_slice(std::slice::from_ref(instance)),
        );

        if index < self.instances.len() {
            self.instances[index] = *instance;
        } else {
            self.instances.push(*instance);
        }
        self.instance_count += 1;
        index
    }

    /// 删除音符（通过将最后一个音符移动到被删除位置）
    pub fn remove_note(&mut self, index: usize) {
        puffin::profile_function!();
        if index >= self.instance_count {
            return;
        }

        self.instance_count -= 1;

        // 如果不是最后一个，将最后一个移动到被删除位置
        if index < self.instance_count {
            let last = self.instances[self.instance_count];
            self.instances[index] = last;

            let offset = index * std::mem::size_of::<crate::NoteInstance>();
            self.queue.write_buffer(
                &self.instance_buffer,
                offset as wgpu::BufferAddress,
                bytemuck::cast_slice(std::slice::from_ref(&last)),
            );
        }

        self.instances.truncate(self.instance_count);
    }

    /// 清空所有音符
    pub fn clear(&mut self) {
        self.instance_count = 0;
        self.instances.clear();
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
}
