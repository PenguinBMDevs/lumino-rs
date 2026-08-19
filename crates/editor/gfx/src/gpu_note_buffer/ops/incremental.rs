//! 事件级增量：保序插入/删除、段写入与 GPU 内部搬移

use super::super::types::GpuNoteBuffer;
use super::move_blocks::compute_move_blocks;

impl GpuNoteBuffer {
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
            self.instance_buffer.inner(),
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
            encoder.copy_buffer_to_buffer(
                self.instance_buffer.inner(),
                src_off,
                &staging,
                0,
                n_bytes,
            );
            encoder.copy_buffer_to_buffer(
                &staging,
                0,
                self.instance_buffer.inner(),
                dst_off,
                n_bytes,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
