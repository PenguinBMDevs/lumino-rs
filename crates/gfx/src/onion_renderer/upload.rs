use super::{OnionNote, OnionRenderer, OnionTrackColors, OnionTrackMask};

impl OnionRenderer {
    /// 上传所有洋葱皮音符到 GPU
    ///
    /// 替换整个音符池内容。传入所有需要显示的其它音轨的音符。
    pub fn upload_notes(
        &mut self,
        notes: &[OnionNote],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let count = notes.len();
        if count == 0 {
            self.note_count = 0;
            self.notes_dirty = true;
            return;
        }

        let mut buffer_rebuilt = false;

        // 按需扩容 note_pool_buffer
        let required = count.next_power_of_two().max(Self::INITIAL_NOTE_CAPACITY);
        if required > self.note_pool_capacity {
            let max_capacity = (self.max_storage_binding as usize
                / std::mem::size_of::<OnionNote>())
            .min(100_000_000);
            let new_capacity = required.min(max_capacity);
            if new_capacity > self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(device, new_capacity);
                self.note_pool_capacity = new_capacity;
                buffer_rebuilt = true;
                tracing::info!(
                    "OnionRenderer: note pool grown to {} ({} MB)",
                    new_capacity,
                    (new_capacity * std::mem::size_of::<OnionNote>()) / (1024 * 1024)
                );
            }
        }

        // 空闲缩容：当实际使用量远低于容量时释放内存
        let shrink_threshold =
            (self.note_pool_capacity as f64 * Self::INDICES_SHRINK_THRESHOLD) as usize;
        if count < shrink_threshold && self.note_pool_capacity > Self::INITIAL_NOTE_CAPACITY * 2 {
            let new_capacity = count.next_power_of_two().max(Self::INITIAL_NOTE_CAPACITY);
            if new_capacity < self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(device, new_capacity);
                self.note_pool_capacity = new_capacity;
                buffer_rebuilt = true;
                tracing::info!(
                    "OnionRenderer: note pool shrunk to {} ({} MB)",
                    new_capacity,
                    (new_capacity * std::mem::size_of::<OnionNote>()) / (1024 * 1024)
                );
            }
        }

        // 按需扩容 instance_indices_buffer（可见音符可能接近总数）
        let required_indices = count;
        if required_indices > self.indices_capacity {
            let new_indices_cap = required_indices
                .next_power_of_two()
                .min(Self::MAX_INDICES_CAPACITY);
            if new_indices_cap > self.indices_capacity {
                self.instance_indices_buffer =
                    Self::create_instance_indices_buffer(device, new_indices_cap);
                self.indices_capacity = new_indices_cap;
                buffer_rebuilt = true;
                tracing::info!(
                    "OnionRenderer: indices buffer grown to {} ({} MB)",
                    new_indices_cap,
                    (new_indices_cap * std::mem::size_of::<u32>()) / (1024 * 1024)
                );
            }
        }

        let upload_count = count.min(self.note_pool_capacity);
        self.note_count = upload_count;
        self.notes_dirty = true;

        queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&notes[..upload_count]),
        );

        // 仅在 buffer 真正被重建时才 rebuild bind group
        if buffer_rebuilt {
            self.rebuild_bind_groups(device);
        }
    }

    /// 上传轨道颜色表
    pub fn upload_track_colors(&self, colors: &OnionTrackColors, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.track_color_buffer,
            0,
            bytemuck::cast_slice(&[*colors]),
        );
    }

    /// 设置轨道掩码
    pub fn upload_track_mask(&mut self, mask: &OnionTrackMask, queue: &wgpu::Queue) {
        if Some(mask) != self.last_track_mask.as_ref() {
            self.last_track_mask = Some(*mask);
        }
        queue.write_buffer(&self.track_mask_buffer, 0, bytemuck::cast_slice(&[*mask]));
    }

    /// 获取当前音符数量
    pub fn note_count(&self) -> usize {
        self.note_count
    }

    /// 获取音符池容量
    pub fn note_pool_capacity(&self) -> usize {
        self.note_pool_capacity
    }

    /// 获取 GPU 内存占用（字节）
    pub fn gpu_memory_usage(&self) -> u64 {
        self.note_pool_buffer.size()
            + self.instance_indices_buffer.size()
            + self.indirect_buffer.size()
            + self.viewport_buffer.size()
            + self.track_mask_buffer.size()
            + self.track_color_buffer.size()
            + self.camera_buffer.size()
    }
}
