//! 洋葱皮按 key 分桶缓存 — 参考 Kiva 瀑布流实现
//!
//! 核心设计：
//! - 256 个 key，每个 key 单独维护按 start_tick 升序排列的 `OnionNote` 数组
//! - 渲染游标交给调用方（如 NoteWorker）持有，桶本身只读，可安全跨线程共享
//! - 正向滚动时调用方从游标处开始扫描，避免每帧全量遍历
//! - 数据来源变化（MIDI 文档加载 / 编辑音轨变更）时增量更新对应音轨的分桶
//!
//! 与现有 SoA 存储的关系：
//! - 输出仍然是 `OnionNote`（16 bytes），复用现有 GPU 管线和着色器
//! - 只是改变了 CPU 侧的组织和过滤方式，把每帧重建变成增量维护 + 游标推进

use lumino_midi_loader::{MidiDocument, NoteInfo};

use crate::OnionNote;

/// 洋葱皮可见性收集参数
///
/// 把 `collect_visible_with_cursor` 的多个视口参数聚合为单个结构体，避免 clippy 参数过多警告，
/// 同时提高调用端可读性。
#[derive(Debug, Clone, Copy)]
pub struct OnionCollectParams {
    /// tick 视口起始
    pub tick_start: f32,
    /// tick 视口结束
    pub tick_end: f32,
    /// key 视口最小值
    pub key_min: u16,
    /// key 视口最大值
    pub key_max: u16,
    /// 上一帧的 tick_start，用于游标重置判断
    pub last_tick_start: f32,
}

impl OnionCollectParams {
    /// 创建新的收集参数
    #[must_use]
    pub fn new(
        tick_start: f32,
        tick_end: f32,
        key_min: u16,
        key_max: u16,
        last_tick_start: f32,
    ) -> Self {
        Self {
            tick_start,
            tick_end,
            key_min,
            key_max,
            last_tick_start,
        }
    }
}

/// 洋葱皮按 key 分桶缓存
///
/// 256 个 key 各自保存按 `start_tick` 升序排列的音符。
/// 桶本身不可变；渲染游标由调用方持有，通过 `collect_visible_with_cursor` 传入。
#[derive(Debug, Clone)]
pub struct OnionSkinBucket {
    /// 按 key 分桶的音符数组
    by_key: Box<[Vec<OnionNote>; 256]>,
    /// 数据版本号，每次 rebuild/update 后递增
    version: u64,
    /// 总音符数量
    total_notes: usize,
}

impl Default for OnionSkinBucket {
    fn default() -> Self {
        Self::new()
    }
}

impl OnionSkinBucket {
    /// 创建空的分桶缓存
    #[must_use]
    pub fn new() -> Self {
        // 安全：Vec::new() 是 const fn，可以用在数组初始化中
        const EMPTY: Vec<OnionNote> = Vec::new();
        Self {
            by_key: Box::new([EMPTY; 256]),
            version: 0,
            total_notes: 0,
        }
    }

    /// 当前数据版本号
    #[inline]
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// 总音符数量
    #[inline]
    #[must_use]
    pub fn total_notes(&self) -> usize {
        self.total_notes
    }

    /// 获取指定 key 的音符数组（只读）
    #[inline]
    #[must_use]
    pub fn key_notes(&self, key: u8) -> &[OnionNote] {
        &self.by_key[key as usize]
    }

    /// 清空所有分桶
    pub fn clear(&mut self) {
        for bucket in self.by_key.iter_mut() {
            bucket.clear();
        }
        self.total_notes = 0;
        self.version = self.version.wrapping_add(1);
    }

    /// 从 MIDI 文档全量重建分桶
    ///
    /// # 参数
    /// - `doc`: MIDI 文档
    /// - `track_filter`: 返回 true 的音轨才会被加入桶中
    /// - `current_track`: 当前编辑音轨，自动排除
    #[must_use]
    pub fn from_midi_document(
        doc: &MidiDocument,
        track_filter: impl Fn(usize) -> bool,
        current_track: usize,
    ) -> Self {
        let mut bucket = Self::new();
        bucket.rebuild_from_midi_document(doc, track_filter, current_track);
        bucket
    }

    /// 从 MIDI 文档重建分桶（会清空已有数据）
    ///
    /// 保留给 `from_midi_document` 和后续增量更新之外的场景使用。
    pub fn rebuild_from_midi_document(
        &mut self,
        doc: &MidiDocument,
        track_filter: impl Fn(usize) -> bool,
        current_track: usize,
    ) {
        self.clear();

        for track_idx in 0..doc.track_count() {
            if track_idx == current_track {
                continue;
            }
            if !track_filter(track_idx) {
                continue;
            }
            self.add_midi_track_notes(doc, track_idx);
        }

        self.sort_all_keys();
        self.total_notes = self.by_key.iter().map(Vec::len).sum();
        self.version = self.version.wrapping_add(1);
    }

    /// 添加/更新一个用户编辑音轨到分桶
    ///
    /// 会先移除该音轨已有的音符，再重新插入。
    pub fn update_user_track<'a>(
        &mut self,
        track_idx: u16,
        notes: impl Iterator<Item = &'a lumino_core::Note>,
    ) {
        self.remove_track(track_idx);

        for note in notes {
            if note.key > 255 {
                continue;
            }
            let start = note.tick as u32;
            let length = note.length.max(0.0) as u32;
            let end = start.saturating_add(length);
            self.by_key[note.key as usize].push(OnionNote::new(
                start,
                end,
                note.key as u8,
                track_idx,
            ));
        }

        // 仅排序被修改过的 key 开销较大（需要记录），这里先保持简单：排序全部 key
        // TODO: 后续可优化为只排序有变更的 key
        self.sort_all_keys();
        self.total_notes = self.by_key.iter().map(Vec::len).sum();
        self.version = self.version.wrapping_add(1);
    }

    /// 从分桶中移除指定音轨的所有音符
    pub fn remove_track(&mut self, track_idx: u16) {
        let mut removed = 0usize;
        for bucket in self.by_key.iter_mut() {
            let before = bucket.len();
            bucket.retain(|n| n.track_idx() != track_idx);
            removed += before - bucket.len();
        }
        if removed > 0 {
            self.total_notes -= removed;
            self.version = self.version.wrapping_add(1);
        }
    }

    /// 收集可见范围内的洋葱皮音符
    ///
    /// 按 key 顺序扫描，利用 `cursor` 复用减少正向滚动时的扫描量。
    /// 输出追加到 `out`，调用方负责清空 out。
    ///
    /// # 参数
    /// - `params`: 视口参数集合
    /// - `cursor`: 每个 key 的渲染游标，由调用方持有并维护
    /// - `track_filter`: 音轨可见性过滤
    pub fn collect_visible_with_cursor(
        &self,
        params: OnionCollectParams,
        cursor: &mut [usize; 256],
        track_filter: impl Fn(u16) -> bool,
        out: &mut Vec<OnionNote>,
    ) {
        let ts = params.tick_start as u32;
        let te = params.tick_end as u32;

        // 时间回退时重置游标
        if params.tick_start < params.last_tick_start {
            cursor.fill(0);
        }

        let key_min = params.key_min.min(255) as usize;
        let key_max = params.key_max.min(255) as usize;

        for (key, key_cursor) in cursor
            .iter_mut()
            .enumerate()
            .take(key_max + 1)
            .skip(key_min)
        {
            let bucket = &self.by_key[key];
            if bucket.is_empty() {
                continue;
            }

            // 推进游标：跳过已经完全离开视口的音符
            while *key_cursor < bucket.len() && bucket[*key_cursor].end_tick <= ts {
                *key_cursor += 1;
            }

            // 从游标开始扫描可见音符
            let mut scan_end = *key_cursor;
            for (i, note) in bucket[*key_cursor..].iter().enumerate() {
                let idx = *key_cursor + i;
                if note.start_tick >= te {
                    break;
                }
                if note.end_tick <= ts {
                    continue;
                }
                if !track_filter(note.track_idx()) {
                    continue;
                }
                out.push(*note);
                scan_end = idx + 1;
            }

            // 游标推进到第一个 start_tick >= te 的音符，供下一帧复用
            while scan_end < bucket.len() && bucket[scan_end].start_tick < te {
                scan_end += 1;
            }
            *key_cursor = scan_end;
        }
    }

    /// 添加 MIDI 文档中指定音轨的音符到分桶
    fn add_midi_track_notes(&mut self, doc: &MidiDocument, track_idx: usize) {
        let track_idx_u16 = track_idx as u16;
        for note in doc.track_notes(track_idx) {
            self.by_key[note.key as usize].push(OnionNote::new(
                note.start_tick,
                note.end_tick(),
                note.key,
                track_idx_u16,
            ));
        }
    }

    /// 对所有 key 的分桶内部按 start_tick 排序
    fn sort_all_keys(&mut self) {
        for bucket in self.by_key.iter_mut() {
            if bucket.len() > 1 {
                bucket.sort_unstable_by_key(|n| (n.start_tick, n.track_idx()));
            }
        }
    }
}

/// 从 `NoteInfo` 切片构建按 key 分桶的临时数据
///
/// 用于单元测试或未来扩展（如直接从 midly 数据构建）
#[must_use]
pub fn build_bucket_from_notes(notes: &[NoteInfo], track_idx: u16) -> OnionSkinBucket {
    let mut bucket = OnionSkinBucket::new();
    for note in notes {
        bucket.by_key[note.key as usize].push(OnionNote::new(
            note.start_tick,
            note.end_tick(),
            note.key,
            track_idx,
        ));
    }
    bucket.sort_all_keys();
    bucket.total_notes = bucket.by_key.iter().map(Vec::len).sum();
    bucket.version = 1;
    bucket
}

#[cfg(test)]
mod tests;
