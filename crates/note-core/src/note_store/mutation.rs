//! NoteStore 变动操作（remove / insert / retain）
//!
//! 这些操作需要移动 SoA 数组元素，复杂度 O(N)，但比 im::Vector 的
//! 对应操作快 10-100x（im::Vector 是 B-tree，需要重新平衡节点）。

use super::{CHUNK_SIZE, Chunk, NoteStore};
use crate::note::Note;

impl NoteStore {
    /// 移除指定索引的音符（O(N) 平均，需要移动后续元素）
    pub fn remove(&mut self, idx: usize) -> Option<Note> {
        let (compacted_idx, local) = self.resolve(idx)?;
        let note = self.chunks[compacted_idx].get(local)?;

        // 将后续所有音符向前移动一位
        let mut current_ci = compacted_idx;
        let current_local = local;

        // 当前块内前移
        for i in current_local..self.chunks[current_ci].len - 1 {
            self.chunks[current_ci].ticks[i] = self.chunks[current_ci].ticks[i + 1];
            self.chunks[current_ci].keys[i] = self.chunks[current_ci].keys[i + 1];
            self.chunks[current_ci].lengths[i] = self.chunks[current_ci].lengths[i + 1];
            self.chunks[current_ci].velocities[i] = self.chunks[current_ci].velocities[i + 1];
            self.chunks[current_ci].channels[i] = self.chunks[current_ci].channels[i + 1];
        }

        // 跨块前移：把下一块的第 0 个元素移到当前块末尾
        while current_ci + 1 < self.chunks.len() {
            let next_has = self.chunks[current_ci + 1].len > 0;
            if next_has {
                let last_idx = self.chunks[current_ci].len - 1;
                self.chunks[current_ci].ticks[last_idx] = self.chunks[current_ci + 1].ticks[0];
                self.chunks[current_ci].keys[last_idx] = self.chunks[current_ci + 1].keys[0];
                self.chunks[current_ci].lengths[last_idx] = self.chunks[current_ci + 1].lengths[0];
                self.chunks[current_ci].velocities[last_idx] =
                    self.chunks[current_ci + 1].velocities[0];
                self.chunks[current_ci].channels[last_idx] =
                    self.chunks[current_ci + 1].channels[0];
            }
            current_ci += 1;
            // 当前块内前移
            for i in 0..self.chunks[current_ci].len - 1 {
                self.chunks[current_ci].ticks[i] = self.chunks[current_ci].ticks[i + 1];
                self.chunks[current_ci].keys[i] = self.chunks[current_ci].keys[i + 1];
                self.chunks[current_ci].lengths[i] = self.chunks[current_ci].lengths[i + 1];
                self.chunks[current_ci].velocities[i] = self.chunks[current_ci].velocities[i + 1];
                self.chunks[current_ci].channels[i] = self.chunks[current_ci].channels[i + 1];
            }
        }

        // 末尾块缩短
        if let Some(last) = self.chunks.last_mut() {
            last.truncate(last.len - 1);
        }
        // 移除空块
        if self.chunks.last().map(|c| c.len) == Some(0) && self.chunks.len() > 1 {
            self.chunks.pop();
        }

        self.rebuild_offsets();
        Some(note)
    }

    /// 在指定索引处插入音符（O(N) 平均）
    pub fn insert(&mut self, idx: usize, note: Note) {
        if idx >= self.total_len {
            self.push_back(note);
            return;
        }

        // 简化实现：如果末尾块有空间，直接后移；否则新建块
        // 先 push_back 一个占位，然后后移
        self.push_back(Note::new(0.0, 0, 0.0));

        // 从末尾向前移动
        for i in (idx + 1..self.total_len).rev() {
            if let Some(prev) = self.get_ref(i - 1) {
                self.modify(i, |note| {
                    note.tick = prev.tick;
                    note.key = prev.key;
                    note.length = prev.length;
                    note.velocity = prev.velocity;
                    note.channel = prev.channel;
                });
            }
        }

        // 写入新音符
        self.modify(idx, |note_ref| {
            note_ref.tick = note.tick;
            note_ref.key = note.key;
            note_ref.length = note.length;
            note_ref.velocity = note.velocity;
            note_ref.channel = note.channel;
        });
    }

    /// retain 语义：保留谓词返回 true 的音符
    pub fn retain(&mut self, mut f: impl FnMut(&Note) -> bool) {
        let mut new_chunks: Vec<Chunk> = Vec::with_capacity(self.chunks.len());
        let mut current_chunk = Chunk::new();

        for chunk in &self.chunks {
            for i in 0..chunk.len {
                let note = Note::from_raw(
                    chunk.ticks[i],
                    chunk.keys[i],
                    chunk.lengths[i],
                    chunk.velocities[i],
                    chunk.channels[i],
                );
                if f(&note) {
                    if current_chunk.len >= CHUNK_SIZE {
                        new_chunks.push(current_chunk);
                        current_chunk = Chunk::new();
                    }
                    current_chunk.push(&note);
                }
            }
        }
        if current_chunk.len > 0 || new_chunks.is_empty() {
            new_chunks.push(current_chunk);
        }

        self.chunks = new_chunks;
        self.rebuild_offsets();
    }
}
