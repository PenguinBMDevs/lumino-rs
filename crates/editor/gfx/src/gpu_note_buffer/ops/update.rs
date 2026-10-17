//! 音符增删改（直接改 GPU 常驻字节，无 CPU 镜像）
//!
//! 2026-10-17 单一权威源整改：GPU 常驻缓冲是音符实例的唯一权威，本模块
//! 所有操作只写 GPU；此前每个操作都额外维护 `self.instances` CPU 镜像，
//! 属冗余拷贝（且 6 亿音符下达 9.6 GB）。

use super::super::types::GpuNoteBuffer;

impl GpuNoteBuffer {
    /// 增量更新单个音符
    ///
    /// 原位写 GPU（不改变计数）。索引越界只告警不写。
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

        let offset = index * std::mem::size_of::<crate::NoteInstance>();
        self.queue.write_buffer(
            self.instance_buffer.inner(),
            offset as wgpu::BufferAddress,
            bytemuck::cast_slice(std::slice::from_ref(instance)),
        );
    }

    /// 批量更新音符（用于编辑操作后的批量更新）
    ///
    /// 原位写 GPU 连续区间（不改变计数）；区间按当前计数截断。
    pub fn update_notes(&mut self, start_index: usize, instances: &[crate::NoteInstance]) {
        puffin::profile_function!();
        if start_index >= self.instance_count || instances.is_empty() {
            return;
        }

        let end_index = (start_index + instances.len()).min(self.instance_count);
        let count = end_index - start_index;

        let offset = start_index * std::mem::size_of::<crate::NoteInstance>();
        self.queue.write_buffer(
            self.instance_buffer.inner(),
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

        self.queue.write_buffer(
            self.instance_buffer.inner(),
            offset as wgpu::BufferAddress,
            bytemuck::cast_slice(std::slice::from_ref(instance)),
        );

        self.instance_count += 1;
        index
    }

    /// 删除音符（swap-remove：把末尾实例搬到被删位置）
    ///
    /// 2026-10-17 单一权威源整改：搬移改在 GPU 内完成，不再从 CPU 镜像取末尾元素。
    /// 旧实现是 CPU 副本唯一无法删除的原因——镜像为空时 `self.instances[instance_count]`
    /// 直接越界 panic。
    ///
    /// 实现注意：**不能**用同 buffer 的 `copy_buffer_to_buffer` —— wgpu 校验
    /// 明确拒绝「Source and destination cannot be the same buffer」（即使区间不重叠，
    /// 实测 wgpu 27 直接报 validation error）。因此复用 [`Self::move_range`] 的
    /// staging 中转路径（与保序删/插同一条已由 `compute_move_blocks` 单测覆盖的搬移）。
    pub fn remove_note(&mut self, index: usize) {
        puffin::profile_function!();
        if index >= self.instance_count {
            return;
        }

        self.instance_count -= 1;

        // 非末位：新的末位实例（index == instance_count）搬到被删位置
        if index < self.instance_count {
            self.move_range(self.instance_count, index, 1);
        }
    }

    /// 保序删除：删除 `[index, index+count)` 区间，后续段 GPU 内部左移
    ///
    /// 主音轨事件级增量专用（`NoteEvent::RemoveAt`）：GPU buffer 顺序与
    /// `notes` 顺序一致，删除后**保持顺序**（区别于 [`Self::remove_note`]
    /// 的 swap-remove 乱序语义）。搬移复用 [`Self::move_range`]
    /// （staging 分块，支持重叠——此处前移不重叠）。全流程无 CPU 拷贝。
    pub fn remove_at(&mut self, index: usize, count: usize) {
        if count == 0 || index >= self.instance_count {
            return;
        }
        let count = count.min(self.instance_count - index);
        let tail = index + count;
        let remaining = self.instance_count - tail;

        // GPU：后续段左移 count（无后续则仅计数变化）
        if remaining > 0 {
            self.move_range(tail, index, remaining);
        }

        self.instance_count -= count;
    }

    /// 清空所有音符
    pub fn clear(&mut self) {
        self.instance_count = 0;
    }
}
