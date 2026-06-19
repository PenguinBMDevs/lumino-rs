use super::{OnionKeyRange, OnionNote, OnionRenderer};
use crate::OnionSkinBucket;
use rayon::prelude::*;

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
    /// - `tick_zoom`: 水平缩放（像素/tick），用于像素高度过滤
    #[allow(clippy::too_many_arguments)]
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
        tick_zoom: f32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let no_change = bucket_version == self.last_bucket_version
            && color_version == self.last_color_version
            && key_min == self.last_key_min
            && key_max == self.last_key_max
            && tick_start == self.last_upload_tick_start
            && tick_end == self.last_upload_tick_end
            && tick_zoom.to_bits() == self.last_zoom_tick.to_bits();
        if no_change {
            return;
        }

        puffin::profile_function!();

        let _perf_start = std::time::Instant::now();

        // 硬上限：与 NoteRenderer 对齐，防止单个 key 的密集区把 pool 撑到 1GB+
        let max_capacity = (self.max_storage_binding as usize / std::mem::size_of::<OnionNote>())
            .min(Self::MAX_NOTE_POOL_CAPACITY);

        // ── Pass 1: 并行计算每个 key 的可见 tick 范围与总数 ──
        // 参照 Kiva / Chikara / Wasabi 的并行 per-key 处理模式。
        let mut upload_key_ranges = [OnionKeyRange::default(); 256];
        let parallel_ranges: Vec<OnionKeyRange> = (key_min..=key_max)
            .into_par_iter()
            .map(|key| {
                let (range_start, range_end) = bucket.find_visible_range(key, tick_start, tick_end);
                OnionKeyRange {
                    start: range_start as u32,
                    end: range_end as u32,
                }
            })
            .collect();
        let mut total_visible = 0usize;
        for (idx, range) in parallel_ranges.iter().enumerate() {
            let key = key_min as usize + idx;
            upload_key_ranges[key] = *range;
            total_visible += (range.end - range.start) as usize;
        }

        // 保存 upload 元数据（兼容模式不再使用，但保留字段避免状态混乱）
        self.upload_key_ranges = upload_key_ranges;
        self.last_upload_tick_start = tick_start;
        self.last_upload_tick_end = tick_end;
        self.last_zoom_tick = tick_zoom;

        if total_visible == 0 {
            self.note_count = 0;
            self.bucket_mode = false;
            self.last_bucket_version = bucket_version;
            self.last_color_version = color_version;
            self.last_key_min = key_min;
            self.last_key_max = key_max;
            self.notes_dirty = true;
            return;
        }

        // ── Pass 2: per-key 像素过滤 + 重叠剔除 ──
        // 参考 Kiva / Chikara 的 note_hide 逻辑：CPU 端直接丢弃被遮挡和亚像素的音符，
        // 将上传数量从数百万降到数千，避免 GPU scan 成为瓶颈。
        let mut per_key_filtered: Vec<Vec<OnionNote>> = (key_min..=key_max)
            .into_par_iter()
            .map(|key| {
                let range = upload_key_ranges[key as usize];
                let mut out = Vec::new();
                if range.start < range.end {
                    let notes = &bucket.key_notes(key)[range.start as usize..range.end as usize];
                    filter_key_notes(notes, tick_end, tick_zoom, track_colors, &mut out);
                }
                out
            })
            .collect();

        let mut total_filtered: usize = per_key_filtered.iter().map(|v| v.len()).sum();

        // ── 超限时按比例裁剪（兜底安全网）──
        // 经过重叠剔除后通常不会触发，但在极端非重叠密集场景下保留最后一道防线。
        if total_filtered > max_capacity {
            let ratio = max_capacity as f64 / total_filtered as f64;
            for notes in &mut per_key_filtered {
                let visible_len = notes.len();
                let clipped_len = (visible_len as f64 * ratio).max(1.0) as usize;
                notes.truncate(clipped_len.min(visible_len));
            }
            total_filtered = per_key_filtered.iter().map(|v| v.len()).sum();
        }

        let note_count_total = bucket.total_notes();
        tracing::debug!(
            "upload_bucket: keys [{},{}] visible={} filtered={} (of {} notes, bv={}, cv={}, zoom={})",
            key_min,
            key_max,
            total_visible,
            total_filtered,
            note_count_total,
            bucket_version,
            color_version,
            tick_zoom,
        );

        // ── Pass 3: flatten 到跨帧复用的 CPU 缓冲 ──
        let mut key_offsets = [0u32; 257];
        self.cpu_note_pool.clear();
        self.cpu_note_pool.reserve(total_filtered);
        let mut offset = 0u32;
        for key in key_min..=key_max {
            let idx = key as usize - key_min as usize;
            key_offsets[key as usize] = offset;
            self.cpu_note_pool.extend_from_slice(&per_key_filtered[idx]);
            offset += per_key_filtered[idx].len() as u32;
        }
        key_offsets[256] = offset;

        let count = offset as usize;

        let mut buffer_rebuilt = false;

        // ── 按需扩容：只 grow，不 shrink ──
        // 参照 NoteRenderer 的硬上限 + 永不缩容策略，彻底消除 32MB↔1GB 震荡。
        // 首次需要扩容时直接跳到 max_capacity，避免 2M→4M→8M→... 的顺序 grow。
        if self.note_pool_capacity < max_capacity {
            let new_capacity = if self.note_pool_capacity == Self::INITIAL_NOTE_CAPACITY {
                max_capacity
            } else {
                count.next_power_of_two().min(max_capacity)
            };
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
        // 重叠剔除后每个 key 的保留音符不再是 bucket 中的连续子区间，
        // 因此回退到兼容模式扫描（GPU 只做当前音轨排除 + pitch/tick 裁剪）。
        self.bucket_mode = false;
        self.last_bucket_version = bucket_version;
        self.last_color_version = color_version;
        self.last_key_min = key_min;
        self.last_key_max = key_max;
        self.notes_dirty = true;

        queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&self.cpu_note_pool[..upload_count]),
        );
        queue.write_buffer(
            &self.key_offsets_buffer,
            0,
            bytemuck::cast_slice(&key_offsets),
        );

        if buffer_rebuilt {
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

        // 硬上限 + 永不缩容（与 bucket 模式保持一致）
        let max_capacity = (self.max_storage_binding as usize / std::mem::size_of::<OnionNote>())
            .min(Self::MAX_NOTE_POOL_CAPACITY);

        // 按需扩容：只 grow，不 shrink；首次直接跳到 max_capacity
        if self.note_pool_capacity < max_capacity {
            let new_capacity = if self.note_pool_capacity == Self::INITIAL_NOTE_CAPACITY {
                max_capacity
            } else {
                count.next_power_of_two().min(max_capacity)
            };
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

/// 对单个 key 的可见音符做像素高度过滤 + 重叠剔除。
///
/// 输入 `notes` 必须已按 `start_tick` 升序排列，且与 `[tick_start, tick_end)` 有重叠。
/// 输出保留的音符写入 `out`，并保持升序。
///
/// 参考 Kiva / Chikara 的 `note_hide` 逻辑：当某个音符的垂直范围被前面已保留的音符
/// 完全覆盖时，直接丢弃，避免上传成千上万被遮挡的音符。
fn filter_key_notes(
    notes: &[OnionNote],
    tick_end: u32,
    tick_zoom: f32,
    track_colors: &[u32],
    out: &mut Vec<OnionNote>,
) {
    out.clear();
    if notes.is_empty() {
        return;
    }

    let min_pixel_height = 1.0f32;
    let min_duration_ticks = if tick_zoom > 0.0 {
        min_pixel_height / tick_zoom
    } else {
        0.0
    };

    let mut max_end: u32 = 0;
    for note in notes {
        let duration = note.end_tick.saturating_sub(note.start_tick);
        if duration == 0 {
            continue;
        }

        // 像素高度过滤：小于 1 像素的音符在屏幕上没有可分辨的高度
        if (duration as f32) < min_duration_ticks {
            continue;
        }

        // 重叠剔除：已被前面保留的音符完全覆盖
        if note.end_tick <= max_end {
            continue;
        }

        // 保留并注入颜色
        max_end = note.end_tick;
        let mut kept = *note;
        if !track_colors.is_empty() {
            let color = track_colors
                .get(note.track_idx() as usize)
                .copied()
                .unwrap_or(0);
            kept.set_color_packed(color);
        }
        out.push(kept);

        // 如果视口已被完全覆盖，后续音符不可能再增加可见区域
        if max_end >= tick_end {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(start_tick: u32, end_tick: u32, pitch: u8, track_idx: u16) -> OnionNote {
        OnionNote::new(start_tick, end_tick, pitch, track_idx)
    }

    #[test]
    fn test_filter_key_notes_empty() {
        let mut out = Vec::new();
        filter_key_notes(&[], 100, 1.0, &[], &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn test_filter_key_notes_no_overlap_keeps_all() {
        let notes = vec![n(0, 10, 60, 0), n(10, 20, 60, 0), n(20, 30, 60, 0)];
        let mut out = Vec::new();
        filter_key_notes(&notes, 100, 1.0, &[], &mut out);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].start_tick, 0);
        assert_eq!(out[1].start_tick, 10);
        assert_eq!(out[2].start_tick, 20);
    }

    #[test]
    fn test_filter_key_notes_full_overlap_keeps_first() {
        let notes = vec![
            n(0, 100, 60, 0),
            n(10, 20, 60, 0),
            n(30, 40, 60, 0),
            n(50, 60, 60, 0),
        ];
        let mut out = Vec::new();
        filter_key_notes(&notes, 100, 1.0, &[], &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].start_tick, 0);
        assert_eq!(out[0].end_tick, 100);
    }

    #[test]
    fn test_filter_key_notes_partial_overlap() {
        let notes = vec![
            n(0, 20, 60, 0),
            n(10, 30, 60, 0),
            n(15, 25, 60, 0),
            n(30, 40, 60, 0),
        ];
        let mut out = Vec::new();
        filter_key_notes(&notes, 100, 1.0, &[], &mut out);
        // 保留 0-20、10-30（扩展覆盖到 30）、30-40
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].start_tick, 0);
        assert_eq!(out[1].start_tick, 10);
        assert_eq!(out[2].start_tick, 30);
    }

    #[test]
    fn test_filter_key_notes_pixel_filter() {
        let notes = vec![
            n(0, 1, 60, 0),   // 1 tick * 0.5 px/tick = 0.5 px -> filtered
            n(10, 12, 60, 0), // 2 ticks * 0.5 = 1.0 px -> kept
            n(20, 21, 60, 0), // 1 tick * 0.5 = 0.5 px -> filtered
        ];
        let mut out = Vec::new();
        filter_key_notes(&notes, 100, 0.5, &[], &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].start_tick, 10);
    }

    #[test]
    fn test_filter_key_notes_zero_duration_skipped() {
        let notes = vec![n(0, 0, 60, 0), n(0, 10, 60, 0), n(10, 10, 60, 0)];
        let mut out = Vec::new();
        filter_key_notes(&notes, 100, 1.0, &[], &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].end_tick, 10);
    }

    #[test]
    fn test_filter_key_notes_stops_when_viewport_covered() {
        let notes = vec![
            n(0, 50, 60, 0),
            n(10, 20, 60, 0),
            n(30, 40, 60, 0),
            n(60, 70, 60, 0),
        ];
        let mut out = Vec::new();
        filter_key_notes(&notes, 50, 1.0, &[], &mut out);
        // 第一个音符已覆盖到 tick_end=50，提前终止
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn test_filter_key_notes_color_injection() {
        let notes = vec![n(0, 10, 60, 1)];
        let colors = vec![0x00000000, 0xFF0000FF];
        let mut out = Vec::new();
        filter_key_notes(&notes, 100, 1.0, &colors, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].color_packed(), 0xFF0000FF);
    }
}
