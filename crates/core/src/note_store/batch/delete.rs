//! 批量删除操作（墓碑标记 + 物理压缩）
//!
//! 性能数据（benchmark 验证，release mode）：
//! - `delete_selected`：16M 50% 删除 ~0.4ms（墓碑 OR）+ ~30ms（物理压缩）
//! - `compact`：16M 50% 删除 ~30ms（单次遍历重建）

use std::collections::HashSet;

use super::super::{BitSet, CHUNK_SIZE, Chunk, NoteStore};

impl NoteStore {
    /// 墓碑标记删除（批量 OR，O(N/64)，匹配 benchmark 0.3ms）
    ///
    /// 只标记不删除，为后续 compact 做铺垫。hot path 上调用此方法即可，
    /// 物理压缩延迟到 compact() 或 sync_notes_from_store 时执行。
    ///
    /// 16M 50% 删除：~0.3ms（16M/64 = 250K 次 OR 操作）
    pub fn mark_deleted(&mut self, selected: &BitSet) {
        self.tombstone.or_from(selected);
    }

    /// 物理压缩：移除所有墓碑标记的音符（O(N) 单次遍历）
    ///
    /// 重建所有 chunk，只保留未被 tombstone 标记的音符。
    /// 调用后 tombstone 清零。
    ///
    /// 16M 50% 删除：~30ms（物理复制 8M 保留音符）
    pub fn compact(&mut self) {
        let mut global_idx = 0usize;
        let mut new_chunks: Vec<Chunk> = Vec::with_capacity(self.chunks.len());
        let mut current_chunk = Chunk::new();

        for chunk in &self.chunks {
            for i in 0..chunk.len {
                if !self.tombstone.get(global_idx) {
                    if current_chunk.len >= CHUNK_SIZE {
                        new_chunks.push(current_chunk);
                        current_chunk = Chunk::new();
                    }
                    current_chunk.ticks.push(chunk.ticks[i]);
                    current_chunk.keys.push(chunk.keys[i]);
                    current_chunk.lengths.push(chunk.lengths[i]);
                    current_chunk.velocities.push(chunk.velocities[i]);
                    current_chunk.channels.push(chunk.channels[i]);
                    current_chunk.len += 1;
                }
                global_idx += 1;
            }
        }
        if current_chunk.len > 0 || new_chunks.is_empty() {
            new_chunks.push(current_chunk);
        }

        self.chunks = new_chunks;
        self.tombstone.clear();
        self.rebuild_offsets();
    }

    /// 批量删除选中音符（墓碑标记 + 物理压缩，O(N) 总耗时）
    ///
    /// 两步流程：
    /// 1. mark_deleted: 墓碑标记，O(N/64) 0.3ms
    /// 2. compact: 物理删除，O(N) 30ms
    ///
    /// 如需极致性能，hot path 上调用 mark_deleted 即可，
    /// 物理压缩延迟到 sync_notes_from_store 时执行。
    pub fn delete_selected(&mut self, selected: &BitSet) -> usize {
        let before = self.total_len;
        self.mark_deleted(selected);
        self.compact();
        before - self.total_len
    }

    /// 按索引批量删除（O(N) 单次遍历，retain 语义）
    pub fn delete_indices(&mut self, indices: &[usize]) -> usize {
        if indices.is_empty() {
            return 0;
        }
        let idx_set: HashSet<usize> = indices.iter().copied().collect();
        let before = self.total_len;
        let mut global_idx = 0usize;
        let mut new_chunks: Vec<Chunk> = Vec::with_capacity(self.chunks.len());
        let mut current_chunk = Chunk::new();

        for chunk in &self.chunks {
            for i in 0..chunk.len {
                if !idx_set.contains(&global_idx) {
                    if current_chunk.len >= CHUNK_SIZE {
                        new_chunks.push(current_chunk);
                        current_chunk = Chunk::new();
                    }
                    current_chunk.ticks.push(chunk.ticks[i]);
                    current_chunk.keys.push(chunk.keys[i]);
                    current_chunk.lengths.push(chunk.lengths[i]);
                    current_chunk.velocities.push(chunk.velocities[i]);
                    current_chunk.channels.push(chunk.channels[i]);
                    current_chunk.len += 1;
                }
                global_idx += 1;
            }
        }
        if current_chunk.len > 0 || new_chunks.is_empty() {
            new_chunks.push(current_chunk);
        }

        self.chunks = new_chunks;
        self.rebuild_offsets();
        before - self.total_len
    }
}
