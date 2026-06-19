use super::{OnionKeyRange, OnionNote, OnionRenderer};
use crate::OnionSkinBucket;

impl OnionRenderer {
    /// 从 `OnionSkinBucket` 上传可见 key 范围的音符池到 GPU
    ///
    /// Bucket 模式核心优化：
    /// - 音符池常驻 GPU，视口变化时只在 CPU 端二分查找每个 key 的可见范围；
    /// - 每帧上传 256 个 `OnionKeyRange`（约 2KB），替代原来的最多 3M 个音符（48MB）上传；
    /// - GPU compute 只扫描可见 key 的可见 tick 范围，而非整个收集后的音符集合。
    ///
    /// 为避免 GPU storage buffer 上限（通常 128MB = ~8M 个 OnionNote），仅上传
    /// 仅上传可见 key 范围 + 可见 tick 范围内的音符。
    ///
    /// 对于 10亿 级数据，单 key 的可见 tick 范围可能仍包含数百万音符，
    /// upload 时通过 `find_visible_range` 做 tick 过滤，大幅减少 GPU storage 压力。
    /// `prepare_cull` 会基于 `last_upload_tick_start/end` 做坐标映射。
    ///
    /// # 参数
    /// - `key_min`, `key_max`: 可见 key 范围（0-255）
    /// - `tick_start`, `tick_end`: 可见 tick 范围
    pub fn upload_bucket(
        &mut self,
        bucket: &OnionSkinBucket,
        bucket_version: u64,
        track_colors: &[u32],
        color_version: u64,
        key_min: u8,
        key_max: u8,
        tick_start: u32,
        tick_end: u32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let no_change = self.bucket_mode
            && bucket_version == self.last_bucket_version
            && color_version == self.last_color_version
            && key_min == self.last_key_min
            && key_max == self.last_key_max
            && tick_start == self.last_upload_tick_start
            && tick_end == self.last_upload_tick_end;
        if no_change {
            return;
        }

        puffin::profile_function!();

        let _perf_start = std::time::Instant::now();

        // 仅 flatten 可见 key + 可见 tick 范围内的音符
        // 用 find_visible_range 做 tick 过滤，而非全量 flatten
        let mut note_pool = Vec::new();
        let mut key_offsets = [0u32; 257];
        let mut upload_key_ranges = [OnionKeyRange::default(); 256];
        let mut offset = 0u32;
        for key in key_min..=key_max {
            key_offsets[key as usize] = offset;
            let bucket_notes = bucket.key_notes(key);
            let (range_start, range_end) = bucket.find_visible_range(key, tick_start, tick_end);
            upload_key_ranges[key as usize] = OnionKeyRange {
                start: range_start as u32,
                end: range_end as u32,
            };
            let visible_notes = &bucket_notes[range_start..range_end];
            if track_colors.is_empty() {
                note_pool.extend_from_slice(visible_notes);
            } else {
                for note in visible_notes {
                    let color = track_colors
                        .get(note.track_idx() as usize)
                        .copied()
                        .unwrap_or(0);
                    let mut colored = *note;
                    colored.set_color_packed(color);
                    note_pool.push(colored);
                }
            }
            offset = note_pool.len() as u32;
        }
        key_offsets[256] = offset;

        // 保存 upload 元数据供 prepare_cull 做坐标映射
        self.upload_key_ranges = upload_key_ranges;
        self.last_upload_tick_start = tick_start;
        self.last_upload_tick_end = tick_end;

        let count = note_pool.len();
        if count == 0 {
            self.note_count = 0;
            self.bucket_mode = true;
            self.last_bucket_version = bucket_version;
            self.last_color_version = color_version;
            self.last_key_min = key_min;
            self.last_key_max = key_max;
            self.notes_dirty = true;
            return;
        }

        let note_count_total = bucket.total_notes();
        tracing::debug!(
            "upload_bucket: flatten keys [{},{}] ({} of {} notes, bv={}, cv={})",
            key_min,
            key_max,
            count,
            note_count_total,
            bucket_version,
            color_version,
        );

        let mut buffer_rebuilt = false;

        // 按需扩容 note_pool_buffer，使用 hysteresis 避免缩容抖动
        let required = count.next_power_of_two().max(Self::INITIAL_NOTE_CAPACITY);
        if required > self.note_pool_capacity {
            // 仅受 GPU storage buffer 限制；如果不够就拉满限制，宁可亏待非可见 key
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

        // instance_indices 容量按可能的最大可见数预留
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
        self.last_key_min = key_min;
        self.last_key_max = key_max;
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

        let elapsed = _perf_start.elapsed();
        tracing::debug!(
            "upload_bucket: done ({} bytes uploaded, took {:?})",
            upload_count * std::mem::size_of::<OnionNote>(),
            elapsed,
        );
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
