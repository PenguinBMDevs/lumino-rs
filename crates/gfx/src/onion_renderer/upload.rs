use super::{OnionNote, OnionRenderer};
use crate::OnionSkinBucket;

impl OnionRenderer {
    /// 从 `OnionSkinBucket` 上传整个音符池到 GPU
    ///
    /// Bucket 模式核心优化：
    /// - 音符池常驻 GPU，视口变化时只在 CPU 端二分查找每个 key 的可见范围；
    /// - 每帧上传 256 个 `OnionKeyRange`（约 2KB），替代原来的最多 3M 个音符（48MB）上传；
    /// - GPU compute 只扫描可见 key 的可见 tick 范围，而非整个收集后的音符集合。
    ///
    /// 仅在 bucket 数据版本或颜色表版本变化时调用；视口变化只更新 `key_ranges_buffer`。
    pub fn upload_bucket(
        &mut self,
        bucket: &OnionSkinBucket,
        bucket_version: u64,
        track_colors: &[u32],
        color_version: u64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let no_change = self.bucket_mode
            && bucket_version == self.last_bucket_version
            && color_version == self.last_color_version;
        if no_change {
            return;
        }

        puffin::profile_function!();

        let mut note_pool = Vec::with_capacity(bucket.total_notes());
        let mut key_offsets = [0u32; 257];
        bucket.flatten_with_key_offsets(&mut note_pool, &mut key_offsets, track_colors);

        let count = note_pool.len();
        if count == 0 {
            self.note_count = 0;
            self.bucket_mode = true;
            self.last_bucket_version = bucket_version;
            self.last_color_version = color_version;
            self.notes_dirty = true;
            return;
        }

        let mut buffer_rebuilt = false;

        // 按需扩容 note_pool_buffer，使用 hysteresis 避免缩容抖动
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
                    "OnionRenderer: bucket note pool grown to {} ({} MB)",
                    new_capacity,
                    (new_capacity * std::mem::size_of::<OnionNote>()) / (1024 * 1024)
                );
            }
        }

        // 空闲缩容：带 hysteresis，避免视口轻微变化导致反复重建 buffer。
        // 只有当使用量低于容量的 12.5% 且容量超过初始值 4 倍时才缩容。
        let shrink_threshold =
            (self.note_pool_capacity as f64 * Self::INDICES_SHRINK_THRESHOLD * 0.5) as usize;
        if count < shrink_threshold && self.note_pool_capacity > Self::INITIAL_NOTE_CAPACITY * 4 {
            let new_capacity = count
                .next_power_of_two()
                .max(Self::INITIAL_NOTE_CAPACITY * 2);
            if new_capacity < self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(device, new_capacity);
                self.note_pool_capacity = new_capacity;
                buffer_rebuilt = true;
                tracing::info!(
                    "OnionRenderer: bucket note pool shrunk to {} ({} MB)",
                    new_capacity,
                    (new_capacity * std::mem::size_of::<OnionNote>()) / (1024 * 1024)
                );
            }
        }

        // instance_indices 容量按可能的最大可见数预留：
        // 一个视口内通常不会同时显示所有音符，但为了避免溢出，至少分配 count。
        let required_indices = count.max(Self::INITIAL_INDICES_CAPACITY);
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
        self.bucket_mode = true;
        self.last_bucket_version = bucket_version;
        self.last_color_version = color_version;
        self.notes_dirty = true;

        queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&note_pool[..upload_count]),
        );
        queue.write_buffer(
            &self.key_offsets_buffer,
            0,
            bytemuck::cast_slice(&key_offsets),
        );

        // 切换到 bucket 模式后 bind group 需要包含 key_offsets/key_ranges
        if buffer_rebuilt || !self.bucket_mode {
            self.rebuild_bind_groups(device);
        }
    }

    /// 上传所有洋葱皮音符到 GPU（兼容模式）
    ///
    /// 替换整个音符池内容。传入所有需要显示的其它音轨的音符。
    /// 颜色已编码在每个音符的 color_packed 字段中。
    ///
    /// 现在保留用于测试和降级路径；生产环境优先使用 [`upload_bucket`]。
    pub fn upload_notes(
        &mut self,
        notes: &[OnionNote],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let count = notes.len();
        if count == 0 {
            self.note_count = 0;
            self.bucket_mode = false;
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

        // 空闲缩容：带 hysteresis，避免视口轻微变化导致反复重建 buffer
        let shrink_threshold =
            (self.note_pool_capacity as f64 * Self::INDICES_SHRINK_THRESHOLD * 0.5) as usize;
        if count < shrink_threshold && self.note_pool_capacity > Self::INITIAL_NOTE_CAPACITY * 4 {
            let new_capacity = count
                .next_power_of_two()
                .max(Self::INITIAL_NOTE_CAPACITY * 2);
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
        let required_indices = count.max(Self::INITIAL_INDICES_CAPACITY);
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
        self.bucket_mode = false;
        self.notes_dirty = true;

        queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&notes[..upload_count]),
        );

        // 兼容模式不需要 key_offsets/key_ranges；buffer 变化时重建 bind group
        if buffer_rebuilt {
            self.rebuild_bind_groups(device);
        }
    }

    /// 获取当前音符数量
    pub fn note_count(&self) -> usize {
        self.note_count
    }

    /// 获取音符池容量
    pub fn note_pool_capacity(&self) -> usize {
        self.note_pool_capacity
    }

    /// 获取实例索引缓冲区容量
    pub fn indices_capacity(&self) -> usize {
        self.indices_capacity
    }

    /// 获取 GPU 内存占用（字节）
    pub fn gpu_memory_usage(&self) -> u64 {
        self.note_pool_buffer.size()
            + self.instance_indices_buffer.size()
            + self.indirect_buffer.size()
            + self.viewport_buffer.size()
            + self.camera_buffer.size()
            + self.key_offsets_buffer.size()
            + self.key_ranges_buffer.size()
    }
}
