//! NoteStore 批量操作（模块根）
//!
//! 子模块：
//! - `r#move`: 批量移动操作（block 并行 + trailing_zeros）
//! - `delete`: 批量删除操作（墓碑标记 + 物理压缩）
//! - `insert`: 批量插入操作（chunk 批量复制）
//! - `modify`: 批量修改操作（力度/门限/音高/位置编辑）

pub(crate) mod delete;
pub(crate) mod insert;
pub(crate) mod modify;
pub(crate) mod r#move;

use std::sync::Arc;

use super::Chunk;
use super::NoteStore;

impl NoteStore {
    /// 克隆为 Arc<NoteStore>（用于 track_notes 缓存共享）
    pub fn clone_arc(&self) -> Arc<NoteStore> {
        Arc::new(self.clone())
    }

    /// 内存占用估算（MB）
    pub fn memory_mb(&self) -> f64 {
        let mut bytes = 0usize;
        for chunk in &self.chunks {
            bytes += chunk.ticks.capacity() * 4;
            bytes += chunk.keys.capacity() * 2;
            bytes += chunk.lengths.capacity() * 4;
            bytes += chunk.velocities.capacity();
            bytes += chunk.channels.capacity();
        }
        bytes += self.chunk_offsets.capacity() * std::mem::size_of::<usize>();
        bytes += self.chunks.capacity() * std::mem::size_of::<Chunk>();
        (bytes as f64) / (1024.0 * 1024.0)
    }
}
