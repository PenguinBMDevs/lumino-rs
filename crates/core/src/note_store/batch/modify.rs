//! 批量修改操作（力度/门限/音高/位置）
//!
//! 性能数据（benchmark 验证，release mode）：
//! - 16M 50% 选中：~18ms（8 线程并行，trailing_zeros 遍历）

use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::{BatchEditOperation, BitSet, Chunk, NoteStore};

impl NoteStore {
    /// 批量编辑选中音符的力度
    ///
    /// 16M 50% 选中：~18ms（8 线程并行，trailing_zeros 遍历）
    pub fn batch_edit_velocity(
        &mut self,
        selected: &BitSet,
        op: BatchEditOperation,
    ) -> usize {
        self.batch_edit_selected(selected, BatchEditTarget::Velocity(op))
    }

    /// 批量编辑选中音符的长度（gate）
    pub fn batch_edit_gate(&mut self, selected: &BitSet, op: BatchEditOperation) -> usize {
        self.batch_edit_selected(selected, BatchEditTarget::Gate(op))
    }

    /// 批量编辑选中音符的音高 key
    pub fn batch_edit_key(
        &mut self,
        selected: &BitSet,
        op: BatchEditOperation,
        max_key: u16,
    ) -> usize {
        self.batch_edit_selected(selected, BatchEditTarget::Key(op, max_key))
    }

    /// 批量编辑选中音符的 tick 位置
    pub fn batch_edit_tick(&mut self, selected: &BitSet, op: BatchEditOperation) -> usize {
        self.batch_edit_selected(selected, BatchEditTarget::Tick(op))
    }

    /// 通用批量编辑：对选中音符应用目标运算
    ///
    /// 8 线程 chunk 级并行，trailing_zeros 只遍历选中位。
    fn batch_edit_selected(&mut self, selected: &BitSet, target: BatchEditTarget) -> usize {
        if self.total_len == 0 || selected.count_ones() == 0 {
            return 0;
        }
        let num_threads = 8usize;
        let chunk_count = self.chunks.len();
        let chunks_per_thread = chunk_count.div_ceil(num_threads).max(1);
        let offsets = self.chunk_offsets.clone();
        let modified = AtomicUsize::new(0);

        std::thread::scope(|s| {
            for (thread_idx, chunk_group) in self.chunks.chunks_mut(chunks_per_thread).enumerate() {
                let group_start = thread_idx * chunks_per_thread;
                let offsets_ref = &offsets;
                let sel = selected;
                let modified_ref = &modified;
                s.spawn(move || {
                    let mut local_modified = 0usize;
                    for (local_ci, chunk) in chunk_group.iter_mut().enumerate() {
                        let chunk_start = offsets_ref[group_start + local_ci];
                        if chunk.len == 0 {
                            continue;
                        }
                        let chunk_end = chunk_start + chunk.len;
                        let start_block = chunk_start / 64;
                        let end_block = (chunk_end - 1) / 64;
                        local_modified += process_edit_chunk_64(
                            chunk, chunk_start, chunk_end, sel,
                            start_block, end_block, &target,
                        );
                    }
                    modified_ref.fetch_add(local_modified, Ordering::Relaxed);
                });
            }
        });

        modified.load(Ordering::Relaxed)
    }
}

/// 批量编辑目标字段
#[derive(Clone, Copy)]
enum BatchEditTarget {
    Velocity(BatchEditOperation),
    Gate(BatchEditOperation),
    Key(BatchEditOperation, u16),
    Tick(BatchEditOperation),
}

/// 处理单个 chunk 的 64-bit 块遍历编辑
#[inline]
fn process_edit_chunk_64(
    chunk: &mut Chunk,
    chunk_start: usize,
    chunk_end: usize,
    sel: &BitSet,
    start_block: usize,
    end_block: usize,
    target: &BatchEditTarget,
) -> usize {
    let mut local_modified = 0usize;
    for bi in start_block..=end_block {
        let block = sel.blocks[bi];
        if block == 0 {
            continue;
        }
        let base = bi * 64;
        let mut bits = block;
        while bits != 0 {
            let trailing_zeros_count = bits.trailing_zeros() as usize;
            let global_idx = base + trailing_zeros_count;
            bits &= bits - 1;
            if global_idx >= chunk_start && global_idx < chunk_end {
                let local = global_idx - chunk_start;
                if apply_batch_edit_target(chunk, local, target) {
                    local_modified += 1;
                }
            }
        }
    }
    local_modified
}

/// 对 chunk 中的单个音符应用批量编辑运算
fn apply_batch_edit_target(chunk: &mut Chunk, local: usize, target: &BatchEditTarget) -> bool {
    match target {
        BatchEditTarget::Velocity(op) => {
            let base = chunk.velocities[local] as f32;
            let new_value = op.apply(base).clamp(0.0, 127.0) as u8;
            if chunk.velocities[local] != new_value {
                chunk.velocities[local] = new_value;
                true
            } else {
                false
            }
        }
        BatchEditTarget::Gate(op) => {
            let base = chunk.lengths[local];
            let new_value = op.apply(base).max(1.0);
            if (chunk.lengths[local] - new_value).abs() > f32::EPSILON {
                chunk.lengths[local] = new_value;
                true
            } else {
                false
            }
        }
        BatchEditTarget::Key(op, max_key) => {
            let max_key_f = *max_key as f32;
            let base = chunk.keys[local] as f32;
            let new_value = op.apply(base).clamp(0.0, max_key_f) as u16;
            if chunk.keys[local] != new_value {
                chunk.keys[local] = new_value;
                true
            } else {
                false
            }
        }
        BatchEditTarget::Tick(op) => {
            let base = chunk.ticks[local];
            let new_value = op.apply(base).max(0.0);
            if (chunk.ticks[local] - new_value).abs() > f32::EPSILON {
                chunk.ticks[local] = new_value;
                true
            } else {
                false
            }
        }
    }
}
