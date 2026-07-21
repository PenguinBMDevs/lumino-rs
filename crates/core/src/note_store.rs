//! Segmented SoA 音符存储（高性能批量操作）
//!
//! 架构：将音符拆分为 SoA 布局的固定大小块（chunk），每块 4096 音符（48KB）。
//!
//! 优势（基于 benchmark 数据）：
//! - 内存：16M 音符 187 MB（vs im::Vector 272 MB，节省 31%）
//! - 插入：1000 音符 1.3ms（vs im::Vector 194ms，快 152x），无需预分配
//! - 批量移动：16M 50% 选中 18.9ms（并行 + trailing_zeros）
//! - 100M 50% 预估 124ms（达标）
//!
//! 设计要点：
//! - 块级并行：批量操作以 chunk 为单位分派到线程，无锁竞争
//! - 墓碑删除：delete 只标记 BitSet，O(64) 批量；undo 只恢复 BitSet
//! - 增长友好：插入只影响末尾块，不触发全量 realloc
//! - 兼容 API：提供与 im::Vector<Note> 相同的 push_back/get/get_mut/iter/len 等
//!
//! 模块拆分：
//! - `bitset`：BitSet 实现（trailing_zeros 优化遍历）
//! - `iter`：NoteStoreIter / NoteStoreRefIter / NoteMut
//! - `batch`：批量并行操作（move / delete / insert）
//! - `tests`：单元测试

mod batch;
mod bitset;
mod iter;
mod mutation;
#[cfg(test)]
mod tests;

pub use bitset::BitSet;
pub use iter::{NoteMut, NoteStoreIter, NoteStoreRefIter};

use crate::note::Note;

/// 块大小：4096 音符 × 12 bytes = 48 KB，适配 L2 缓存
pub(crate) const CHUNK_SIZE: usize = 4096;

/// SoA 布局的音符块
pub(crate) struct Chunk {
    pub(crate) ticks: Vec<f32>,
    pub(crate) keys: Vec<u16>,
    pub(crate) lengths: Vec<f32>,
    pub(crate) velocities: Vec<u8>,
    pub(crate) channels: Vec<u8>,
    pub(crate) len: usize,
}

impl Chunk {
    fn new() -> Self {
        Self {
            ticks: Vec::with_capacity(CHUNK_SIZE),
            keys: Vec::with_capacity(CHUNK_SIZE),
            lengths: Vec::with_capacity(CHUNK_SIZE),
            velocities: Vec::with_capacity(CHUNK_SIZE),
            channels: Vec::with_capacity(CHUNK_SIZE),
            len: 0,
        }
    }

    fn push(&mut self, note: &Note) {
        self.ticks.push(note.tick);
        self.keys.push(note.key);
        self.lengths.push(note.length);
        self.velocities.push(note.velocity);
        self.channels.push(note.channel);
        self.len += 1;
    }

    pub(crate) fn get(&self, local_idx: usize) -> Option<Note> {
        if local_idx < self.len {
            Some(Note::from_raw(
                self.ticks[local_idx],
                self.keys[local_idx],
                self.lengths[local_idx],
                self.velocities[local_idx],
                self.channels[local_idx],
            ))
        } else {
            None
        }
    }

    fn get_ref(&self, local_idx: usize) -> Option<NoteView> {
        if local_idx < self.len {
            Some(NoteView {
                tick: self.ticks[local_idx],
                key: self.keys[local_idx],
                length: self.lengths[local_idx],
                velocity: self.velocities[local_idx],
                channel: self.channels[local_idx],
            })
        } else {
            None
        }
    }

    fn apply_to(&mut self, local_idx: usize, f: impl FnOnce(&mut Note)) {
        if local_idx < self.len {
            let mut n = Note::from_raw(
                self.ticks[local_idx],
                self.keys[local_idx],
                self.lengths[local_idx],
                self.velocities[local_idx],
                self.channels[local_idx],
            );
            f(&mut n);
            self.ticks[local_idx] = n.tick;
            self.keys[local_idx] = n.key;
            self.lengths[local_idx] = n.length;
            self.velocities[local_idx] = n.velocity;
            self.channels[local_idx] = n.channel;
        }
    }

    fn truncate(&mut self, new_len: usize) {
        self.ticks.truncate(new_len);
        self.keys.truncate(new_len);
        self.lengths.truncate(new_len);
        self.velocities.truncate(new_len);
        self.channels.truncate(new_len);
        self.len = new_len;
    }

    fn remaining(&self) -> usize {
        CHUNK_SIZE - self.len
    }
}

/// 音符只读视图（避免构造 Note 结构体的开销）
#[derive(Debug, Clone, Copy)]
pub struct NoteView {
    pub tick: f32,
    pub key: u16,
    pub length: f32,
    pub velocity: u8,
    pub channel: u8,
}

impl From<Note> for NoteView {
    fn from(n: Note) -> Self {
        Self {
            tick: n.tick,
            key: n.key,
            length: n.length,
            velocity: n.velocity,
            channel: n.channel,
        }
    }
}

impl From<&Note> for NoteView {
    /// 从 &Note 零 clone 构造 NoteView（字段全部 Copy）
    ///
    /// 用于 im::Vector 路径下 `for_each_note_view` 等场景，避免先 clone Note
    /// 再消耗的冗余开销。
    fn from(n: &Note) -> Self {
        Self {
            tick: n.tick,
            key: n.key,
            length: n.length,
            velocity: n.velocity,
            channel: n.channel,
        }
    }
}

impl From<NoteView> for Note {
    fn from(r: NoteView) -> Self {
        Note::from_raw(r.tick, r.key, r.length, r.velocity, r.channel)
    }
}

/// Segmented SoA 音符存储
///
/// 提供与 `im::Vector<Note>` 兼容的 API 子集，同时支持高性能批量操作。
/// 内部用分块 SoA 布局，块大小 4096（48KB），适配 L2 缓存。
pub struct NoteStore {
    pub(crate) chunks: Vec<Chunk>,
    /// 前缀和：chunk_offsets[i] = 前 i 个 chunk 的总音符数
    pub(crate) chunk_offsets: Vec<usize>,
    pub(crate) total_len: usize,
}

impl std::fmt::Debug for NoteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoteStore")
            .field("chunks", &self.chunks.len())
            .field("total_len", &self.total_len)
            .field("memory_mb", &format_args!("{:.2}", self.memory_mb()))
            .finish()
    }
}

impl Default for NoteStore {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            chunk_offsets: vec![0],
            total_len: 0,
        }
    }
}

impl NoteStore {
    /// 创建空存储
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 im::Vector 转换（用于从旧架构迁移）
    pub fn from_im_vector(v: &im::Vector<Note>) -> Self {
        let mut store = Self::with_capacity(v.len());
        for note in v.iter() {
            store.push_back(note.clone());
        }
        store
    }

    /// 转换回 im::Vector（用于兼容旧代码）
    pub fn to_im_vector(&self) -> im::Vector<Note> {
        let mut v = im::Vector::new();
        for i in 0..self.total_len {
            if let Some(note) = self.get(i) {
                v.push_back(note);
            }
        }
        v
    }

    /// 预分配容量（只分配 Vec 容量，不创建空 chunk，避免 offsets 不同步）
    pub fn with_capacity(capacity: usize) -> Self {
        let chunk_count = capacity.div_ceil(CHUNK_SIZE);
        Self {
            chunks: Vec::with_capacity(chunk_count),
            chunk_offsets: vec![0],
            total_len: 0,
        }
    }

    /// 音符总数
    pub fn len(&self) -> usize {
        self.total_len
    }

    pub fn is_empty(&self) -> bool {
        self.total_len == 0
    }

    /// 清空所有音符
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.chunk_offsets = vec![0];
        self.total_len = 0;
    }

    /// 追加音符到末尾
    pub fn push_back(&mut self, note: Note) {
        // 末尾块是否有空间
        if let Some(last) = self.chunks.last_mut() {
            if last.remaining() > 0 {
                last.push(&note);
                self.total_len += 1;
                self.update_last_offset();
                return;
            }
        }
        // 需要新块
        let mut chunk = Chunk::new();
        chunk.push(&note);
        self.chunks.push(chunk);
        self.total_len += 1;
        self.update_last_offset();
    }

    /// 批量追加（避免多次 push_back 的块检查开销）
    pub fn extend_from_slice(&mut self, notes: &[Note]) {
        for note in notes {
            self.push_back(note.clone());
        }
    }

    /// 更新最后一个 chunk_offset
    fn update_last_offset(&mut self) {
        let total = self.total_len;
        if self.chunk_offsets.len() == self.chunks.len() {
            self.chunk_offsets.push(total);
        } else if let Some(last) = self.chunk_offsets.last_mut() {
            *last = total;
        }
    }

    /// 重建所有 chunk_offsets（插入/删除后调用）
    pub(crate) fn rebuild_offsets(&mut self) {
        self.chunk_offsets.clear();
        self.chunk_offsets.push(0);
        let mut acc = 0;
        for chunk in &self.chunks {
            acc += chunk.len;
            self.chunk_offsets.push(acc);
        }
        self.total_len = acc;
    }

    /// 全局索引 → (chunk_idx, local_idx)，二分查找 O(log N)
    pub(crate) fn resolve(&self, global_idx: usize) -> Option<(usize, usize)> {
        if global_idx >= self.total_len {
            return None;
        }
        // partition_point 找到第一个 > global_idx 的位置，前一个就是 chunk
        let ci = self
            .chunk_offsets
            .partition_point(|&o| o <= global_idx)
            .saturating_sub(1);
        let local = global_idx - self.chunk_offsets[ci];
        Some((ci, local))
    }

    /// 获取音符副本
    pub fn get(&self, idx: usize) -> Option<Note> {
        let (ci, local) = self.resolve(idx)?;
        self.chunks[ci].get(local)
    }

    /// 获取音符只读视图（避免构造 Note 结构体）
    pub fn get_ref(&self, idx: usize) -> Option<NoteView> {
        let (ci, local) = self.resolve(idx)?;
        self.chunks[ci].get_ref(local)
    }

    /// 修改单个音符
    pub fn get_mut(&mut self, idx: usize) -> Option<NoteMut<'_>> {
        let (ci, local) = self.resolve(idx)?;
        Some(NoteMut {
            chunk: &mut self.chunks[ci],
            local_idx: local,
        })
    }

    /// 索引访问（panic 版本，兼容旧代码 `notes[i]`）
    pub fn index(&self, idx: usize) -> Note {
        self.get(idx).expect("NoteStore index out of bounds")
    }

    /// 修改单个音符（回调式，避免中间 Note 结构体）
    pub fn modify(&mut self, idx: usize, f: impl FnOnce(&mut Note)) -> bool {
        let (ci, local) = match self.resolve(idx) {
            Some(v) => v,
            None => return false,
        };
        self.chunks[ci].apply_to(local, f);
        true
    }

    /// 迭代器（返回 Note 副本，每个音符 clone 一次）
    pub fn iter(&self) -> NoteStoreIter<'_> {
        NoteStoreIter {
            store: self,
            idx: 0,
        }
    }

    /// 迭代器（返回 NoteView 引用，Copy 语义零 clone）
    ///
    /// 16M 音符场景下比 `iter()` 节省 ~80ms 的 Note 结构体构造开销。
    pub fn iter_refs(&self) -> NoteStoreRefIter<'_> {
        NoteStoreRefIter {
            store: self,
            idx: 0,
        }
    }

    /// 回调式遍历 NoteView（无 Note 副本，最高性能）
    ///
    /// 比 `iter_refs()` 更快——直接遍历 SoA 数组，无 NoteView 结构体构造。
    /// 用于空间索引构建等大数据量场景。
    pub fn for_each_ref(&self, mut f: impl FnMut(usize, NoteView)) {
        let mut global = 0usize;
        for chunk in &self.chunks {
            for i in 0..chunk.len {
                f(
                    global,
                    NoteView {
                        tick: chunk.ticks[i],
                        key: chunk.keys[i],
                        length: chunk.lengths[i],
                        velocity: chunk.velocities[i],
                        channel: chunk.channels[i],
                    },
                );
                global += 1;
            }
        }
    }
}

impl Clone for NoteStore {
    fn clone(&self) -> Self {
        let chunks: Vec<Chunk> = self
            .chunks
            .iter()
            .map(|c| Chunk {
                ticks: c.ticks.clone(),
                keys: c.keys.clone(),
                lengths: c.lengths.clone(),
                velocities: c.velocities.clone(),
                channels: c.channels.clone(),
                len: c.len,
            })
            .collect();
        Self {
            chunks,
            chunk_offsets: self.chunk_offsets.clone(),
            total_len: self.total_len,
        }
    }
}
