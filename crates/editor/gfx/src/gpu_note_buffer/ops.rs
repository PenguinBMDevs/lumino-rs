//! GPU 音符缓冲区公开操作

use super::types::GpuNoteBuffer;

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
            &self.instance_buffer,
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
            &self.instance_buffer,
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

    /// 增量更新单个音符
    ///
    /// 流式模式（CPU 镜像已清空）下直接写 GPU；非流式模式同步 CPU 镜像。
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

        // 上传单个实例
        let offset = index * std::mem::size_of::<crate::NoteInstance>();
        self.queue.write_buffer(
            &self.instance_buffer,
            offset as wgpu::BufferAddress,
            bytemuck::cast_slice(std::slice::from_ref(instance)),
        );

        // 同步 CPU 缓存（非流式模式存在镜像）
        if !self.instances.is_empty() {
            self.instances[index] = *instance;
        }
    }

    /// 批量更新音符（用于编辑操作后的批量更新）
    ///
    /// 流式模式（CPU 镜像已清空）下直接写 GPU；非流式模式同步 CPU 镜像。
    pub fn update_notes(&mut self, start_index: usize, instances: &[crate::NoteInstance]) {
        puffin::profile_function!();
        if start_index >= self.instance_count || instances.is_empty() {
            return;
        }

        let end_index = (start_index + instances.len()).min(self.instance_count);
        let count = end_index - start_index;

        // 批量上传 GPU（流式模式安全）
        let offset = start_index * std::mem::size_of::<crate::NoteInstance>();
        self.queue.write_buffer(
            &self.instance_buffer,
            offset as wgpu::BufferAddress,
            bytemuck::cast_slice(&instances[..count]),
        );

        // 同步 CPU 缓存（非流式模式存在镜像）
        if !self.instances.is_empty() {
            for (index, instance) in instances[..count].iter().enumerate() {
                self.instances[start_index + index] = *instance;
            }
        }
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

    /// 保序删除：删除 `[index, index+count)` 区间，后续段 GPU 内部左移
    ///
    /// 主音轨事件级增量专用（`NoteEvent::RemoveAt`）：GPU buffer 顺序与
    /// `notes` 顺序一致，删除后**保持顺序**（区别于 [`Self::remove_note`]
    /// 的 swap-remove 乱序语义）。搬移复用 [`Self::move_range`]
    /// （staging 分块，支持重叠——此处前移不重叠）。
    ///
    /// CPU 镜像（`self.instances`，主音轨模式存在）同步 drain；
    /// 流式模式（洋葱皮）镜像为空时跳过镜像操作。
    pub fn remove_at(&mut self, index: usize, count: usize) {
        if count == 0 || index >= self.instance_count {
            return;
        }
        let count = count.min(self.instance_count - index);
        let tail = index + count;
        let remaining = self.instance_count - tail;

        // CPU 镜像同步（流式模式镜像为空则跳过）
        if !self.instances.is_empty() {
            self.instances.drain(index..tail);
        }

        // GPU：后续段左移 count（无后续则仅计数变化）
        if remaining > 0 {
            self.move_range(tail, index, remaining);
        }

        self.instance_count -= count;
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

    // ── 洋葱皮事件级增量专用（流式模式安全：不触碰已清空的 CPU 缓存）──
    // 流式模式 instances 已清空：update_notes 等会索引空 Vec panic，且按 count 截断。

    /// 保序插入：在 `index` 处插入 `instances`，后续段 GPU 内部右移
    ///
    /// 主音轨可见列表 diff 增量专用（`NoteEvent::Insert`）：与 [`Self::remove_at`]
    /// 互为逆操作。搬移复用 [`Self::move_range`]（staging 分块，支持重叠——
    /// 此处右移从尾向前，安全）。
    ///
    /// CPU 镜像（`self.instances`，主音轨模式存在）同步 splice；
    /// 流式模式（洋葱皮）镜像为空时跳过镜像操作。
    pub fn insert_at(&mut self, index: usize, instances: &[crate::NoteInstance]) {
        puffin::profile_function!();
        if instances.is_empty() {
            return;
        }
        let index = index.min(self.instance_count);
        let new_count = self.instance_count + instances.len();

        // 检查是否需要扩容
        if new_count > self.capacity && !self.grow(new_count) {
            tracing::error!(
                "GpuNoteBuffer: insert_at grow failed, dropping {} instances",
                instances.len()
            );
            return;
        }

        // GPU：后续段右移 len（无后续则仅写段）
        let tail = self.instance_count - index;
        if tail > 0 {
            self.move_range(index, index + instances.len(), tail);
        }
        // 写入新段（write_buffer 在 move_range 的 submit 之后入队，顺序保证）
        self.write_segment(index, instances);

        // CPU 镜像同步（流式模式镜像为空则跳过）
        if !self.instances.is_empty() {
            self.instances
                .splice(index..index, instances.iter().copied());
        }

        self.instance_count = new_count;
    }

    /// 段内写：将 `instances` 写入 `offset` 处（不更新计数、不触碰 CPU 缓存）
    /// 写前调用方需保证容量充足（grow 后）且目标区间不与未搬移的后续段重叠。
    pub fn write_segment(&mut self, offset: usize, instances: &[crate::NoteInstance]) {
        if instances.is_empty() {
            return;
        }
        let offset_bytes = offset * std::mem::size_of::<crate::NoteInstance>();
        self.queue.write_buffer(
            &self.instance_buffer,
            offset_bytes as wgpu::BufferAddress,
            bytemuck::cast_slice(instances),
        );
    }

    /// 设置实例计数（变长段替换完成后更新，供 cull uniform / 段表使用）
    pub fn set_instance_count(&mut self, count: usize) {
        self.instance_count = count;
    }

    /// GPU 内部搬移：`[src, src+count)` → `dst`（同 buffer，支持重叠与任意方向）
    /// 无 CPU 镜像下的后续段移动（COPY_SRC/DST 已声明）。staging 分块 + 方向序
    /// （后移从尾向前、前移从头向后），规避 copy 重叠 UB；块 = 100 万实例(16MB)。
    pub fn move_range(&mut self, src: usize, dst: usize, count: usize) {
        const MOVE_BLOCK: usize = 1_000_000;

        if count == 0 || src == dst {
            return;
        }

        // 分块序列（纯函数，逻辑已单测）
        let blocks = compute_move_blocks(src, dst, count, MOVE_BLOCK);
        if blocks.is_empty() {
            return;
        }

        let instance_size = std::mem::size_of::<crate::NoteInstance>() as u64;
        // staging 按实际最大块分配：小搬移（如插入单个音符）不分配 16MB 中转区
        let staging_size = (count.min(MOVE_BLOCK) as u64) * instance_size;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_note_buffer_move_staging"),
            size: staging_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu_note_buffer_move"),
            });

        for (b_src, b_dst, n) in blocks {
            let n_bytes = (n as u64) * instance_size;
            let src_off = (b_src as u64) * instance_size;
            let dst_off = (b_dst as u64) * instance_size;
            encoder.copy_buffer_to_buffer(&self.instance_buffer, src_off, &staging, 0, n_bytes);
            encoder.copy_buffer_to_buffer(&staging, 0, &self.instance_buffer, dst_off, n_bytes);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

/// 计算搬移分块序列（纯函数，可单测）
///
/// 返回按执行顺序的 `(src, dst, n)` 列表：每块源/目标区间互不重叠
/// （staging 中转后安全）；方向序保证不覆盖未搬源块（后移从尾向前、前移从头向后）。
/// 正确性由测试用 `Vec::copy_within`（标准 memmove 语义）对照验证。
pub fn compute_move_blocks(
    src: usize,
    dst: usize,
    count: usize,
    max_block: usize,
) -> Vec<(usize, usize, usize)> {
    if count == 0 || src == dst || max_block == 0 {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    if dst > src {
        // 后移：从尾部向前（目标区在源区之后，先搬最后块不会覆盖未搬的源）
        let mut remaining = count;
        let mut s_end = src + count;
        let mut d_end = dst + count;
        while remaining > 0 {
            let n = remaining.min(max_block);
            s_end -= n;
            d_end -= n;
            blocks.push((s_end, d_end, n));
            remaining -= n;
        }
    } else {
        // 前移：从头部向后（目标区在源区之前，先搬最前块不会覆盖未搬的源）
        let mut s = src;
        let mut d = dst;
        let mut remaining = count;
        while remaining > 0 {
            let n = remaining.min(max_block);
            blocks.push((s, d, n));
            s += n;
            d += n;
            remaining -= n;
        }
    }
    blocks
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
