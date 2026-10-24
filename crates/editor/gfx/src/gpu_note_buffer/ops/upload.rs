//! 缓冲区创建与（全量/流式）上传

use crate::epoch::GpuEpoch;

use super::super::types::GpuNoteBuffer;

impl GpuNoteBuffer {
    /// 初始容量
    pub const INITIAL_CAPACITY: usize = 1024;

    /// 扩容因子
    pub const GROWTH_FACTOR: usize = 2;

    /// 软件容量上限（`usize::MAX` = 不额外截断）。
    ///
    /// 真实容量由 `max_buffer_size / 12` 决定；单 storage binding 2GB-1
    /// 的限制由 `ChunkLayout` 按 chunk 切片解决，不再把总容量钉死在 2GB。
    /// 12B × 310M ≈ 3.73GB，4GB buffer 可容纳。
    pub const MAX_CAPACITY: usize = usize::MAX;

    /// 创建新的 GPU 音符缓冲区
    ///
    /// `epoch`：创建时所属 GPU 代次（REND-019 PR-B 登记）。
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, epoch: GpuEpoch) -> Self {
        let instance_buffer = Self::create_buffer(device, Self::INITIAL_CAPACITY);

        let max_buffer_size = device.limits().max_buffer_size;
        // 12B 源缓冲由 NoteRenderer 按 2GB binding 分 chunk 绑定；
        // 总容量只受 max_buffer_size 约束（否则 310M 永远进不来）。
        let max_bytes = max_buffer_size as usize;
        // 硬件限制仍需保留（wgpu 物理限制无法绕过），但不再叠加软件 MAX_CAPACITY 截断
        let max_capacity = max_bytes / std::mem::size_of::<crate::NoteInstance>();

        Self {
            instance_buffer,
            capacity: Self::INITIAL_CAPACITY,
            instance_count: 0,
            max_capacity,
            dropped_instances: 0,
            device: std::sync::Arc::new(device.clone()),
            queue: std::sync::Arc::new(queue.clone()),
            epoch,
        }
    }

    /// 批量上传所有音符（初始化时使用）
    ///
    /// 优化：
    /// 1. 使用 extend_from_slice 替代 to_vec，避免重复分配
    /// 2. 预分配 capacity，减少重新分配
    ///
    /// 不叠加软件截断；容量只受硬件 `max_capacity` 限制（在 `new()` 中计算）。
    /// 超限时早退并记录 `dropped_instances`，不得向旧 buffer 越界写。
    pub fn upload_all(&mut self, instances: &[crate::NoteInstance]) {
        puffin::profile_function!();
        if instances.is_empty() {
            self.instance_count = 0;
            self.dropped_instances = 0;
            return;
        }

        // 检查是否需要扩容。
        // DEBT-03 #120：扩容失败（超硬件上限）必须早退——旧实现丢弃 grow 返回值后
        // 无条件全量 write_buffer，会向旧的小 buffer 越界写 → wgpu 校验错误。
        if instances.len() > self.capacity && !self.grow(instances.len()) {
            tracing::error!(
                "GpuNoteBuffer: upload_all 扩容失败（需要 {}，容量 {}，硬件上限 {}），跳过本次上传",
                instances.len(),
                self.capacity,
                self.max_capacity
            );
            self.dropped_instances = instances.len();
            return;
        }
        self.dropped_instances = 0;

        // 用户硬约束：不再截断——grow 已在超硬件上限时明确失败并早退
        let upload_count = instances.len();
        self.instance_count = upload_count;

        // 直接上传输入 slice，不再落 CPU 镜像（2026-10-17 单一权威源整改）。
        // 旧实现先整份 memcpy 进 `self.instances` 再从镜像上传——`write_buffer`
        // 本身就是 memcpy，落镜像纯属多一份 CPU 常驻（16M 音符 = 256MB），
        // 且在 6 亿音符下把 CPU 峰值推到 30–40 GB。
        self.queue.write_buffer(
            self.instance_buffer.inner(),
            0,
            bytemuck::cast_slice(&instances[..upload_count]),
        );

        tracing::debug!("Uploading {} notes", upload_count);
    }

    /// 流式上传：开始一次流式上传会话
    ///
    /// 重置 `instance_count = 0`，准备接受分块 append（分块直接写 GPU，无 CPU 镜像）。
    ///
    /// 历史（2026-08-05，6 亿音符 CPU 峰值 30–40 GB）：当时只有本流式路径跳过
    /// CPU 全量副本；2026-10-17 整改后**所有**上传/增量路径都不再持有 CPU 镜像，
    /// 本方法只剩「重置计数」一件事。
    pub fn begin_streaming_upload(&mut self) {
        puffin::profile_function!();
        self.instance_count = 0;
        self.dropped_instances = 0;
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
            // 逐块 error 会刷屏；累计到 finish_streaming_upload 报一次。
            self.dropped_instances += chunk.len();
            tracing::debug!(
                "GpuNoteBuffer: streaming_append grow failed, dropping {} instances (total dropped {})",
                chunk.len(),
                self.dropped_instances
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
        if self.dropped_instances > 0 {
            tracing::warn!(
                "GpuNoteBuffer: 流式上传完成：GPU 保留 {} 个实例，丢弃 {} 个（单 buffer 16B/实例的 2GB binding 物理上限={}）。\
                 需要完整显示请启用多 buffer 分段（待实现）。",
                self.instance_count,
                self.dropped_instances,
                self.max_capacity
            );
        } else {
            tracing::debug!(
                "Streaming upload finished: {} instances on GPU",
                self.instance_count
            );
        }
    }
}
