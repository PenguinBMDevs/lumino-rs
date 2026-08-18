//! 音符增删改（CPU 镜像同步 + GPU 上传）

use super::super::types::GpuNoteBuffer;

impl GpuNoteBuffer {
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
            self.instance_buffer.inner(),
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
            self.instance_buffer.inner(),
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
            self.instance_buffer.inner(),
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
                self.instance_buffer.inner(),
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
}
