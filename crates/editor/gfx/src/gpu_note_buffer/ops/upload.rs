//! 缓冲区创建与（全量/流式）上传

use super::super::types::GpuNoteBuffer;

impl GpuNoteBuffer {
    /// 初始容量
    pub const INITIAL_CAPACITY: usize = 1024;

    /// 扩容因子
    pub const GROWTH_FACTOR: usize = 2;

    /// 最大容量
    ///
    /// 用户硬约束：不得限制 GPU 内存使用。设为 usize::MAX 表示无限制，
    /// 实际容量仅受 wgpu 硬件限制（max_storage_buffer_binding_size / max_buffer_size）。
    pub const MAX_CAPACITY: usize = usize::MAX;

    /// 创建新的 GPU 音符缓冲区
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let instance_buffer = Self::create_buffer(device, Self::INITIAL_CAPACITY);

        let max_binding_size = device.limits().max_storage_buffer_binding_size as u64;
        let max_buffer_size = device.limits().max_buffer_size;
        let max_bytes = max_binding_size.min(max_buffer_size) as usize;
        // 硬件限制仍需保留（wgpu 物理限制无法绕过），但不再叠加软件 MAX_CAPACITY 截断
        let max_capacity = max_bytes / std::mem::size_of::<crate::NoteInstance>();

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
    ///
    /// 用户硬约束：不得限制 GPU 内存使用——删除 max_capacity 截断逻辑，
    /// 实际容量仅受 wgpu 硬件限制（在 new() 中已计算 max_capacity）。
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

        // 用户硬约束：不再截断——若超出硬件限制会在 grow() 时失败并报错
        let upload_count = instances.len();
        if instances.len() > self.max_capacity {
            tracing::error!(
                "GpuNoteBuffer: instance count {} exceeds hardware max_capacity {} — \
                 wgpu 硬件限制无法绕过，需要分 buffer 上传架构改造",
                instances.len(),
                self.max_capacity
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
            self.instance_buffer.inner(),
            0,
            bytemuck::cast_slice(&self.instances),
        );

        tracing::debug!("Uploading {} notes", upload_count);
    }

    /// 流式上传：开始一次流式上传会话
    ///
    /// 重置 instance_count=0，准备接受分块 append。
    /// 不清空 self.instances（如果存在），因为流式模式下不再使用 CPU 全量副本。
    ///
    /// 性能优化（6 亿音符 CPU 峰值问题 2026-08-05）：
    /// 旧 `upload_all` 维护 `self.instances: Vec<NoteInstance>` CPU 全量副本（9.6 GB @ 6 亿音符），
    /// 导致 CPU 峰值 30-40 GB。流式模式跳过 CPU 副本，分块直接上传 GPU。
    pub fn begin_streaming_upload(&mut self) {
        puffin::profile_function!();
        self.instance_count = 0;
        // 释放 CPU 缓存——流式模式不再需要全量副本
        self.instances.clear();
        self.instances.shrink_to_fit();
    }

    /// 流式上传：追加一块音符实例到 GPU buffer
    ///
    /// 自动扩容。每块大小建议 ≤ 800 万实例（128 MB）以平衡传输效率与单次 write_buffer 内存峰值。
    /// 不维护 CPU 全量副本——直接 write_buffer 到对应 offset。
    pub fn streaming_append(&mut self, chunk: &[crate::NoteInstance]) {
        puffin::profile_function!();
        if chunk.is_empty() {
            return;
        }

        let new_count = self.instance_count + chunk.len();

        // 检查是否需要扩容
        if new_count > self.capacity && !self.grow(new_count) {
            tracing::error!(
                "GpuNoteBuffer: streaming_append grow failed, dropping {} instances",
                chunk.len()
            );
            return;
        }

        // 直接上传到 GPU buffer 的对应 offset——不维护 CPU 副本
        let offset_bytes = self.instance_count * std::mem::size_of::<crate::NoteInstance>();
        self.queue.write_buffer(
            self.instance_buffer.inner(),
            offset_bytes as wgpu::BufferAddress,
            bytemuck::cast_slice(chunk),
        );

        self.instance_count = new_count;
    }

    /// 流式上传：结束会话
    ///
    /// 仅做最终校验和日志，GPU 数据已在 streaming_append 中就位。
    pub fn finish_streaming_upload(&self) {
        puffin::profile_function!();
        tracing::debug!(
            "Streaming upload finished: {} instances on GPU",
            self.instance_count
        );
    }
}
