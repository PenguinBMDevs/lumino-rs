//! 批量移动操作（块级并行 + trailing_zeros 选中位遍历）
//!
//! 性能数据（benchmark 验证，release mode）：
//! - `batch_move_parallel`：16M 50% 选中 ~18ms（8 线程, trailing_zeros）

use bit_vec::BitVec;

use super::super::{BitSet, Chunk, NoteStore};

impl NoteStore {
    /// 批量移动选中音符（trailing_zeros 跳过非选中位，8 线程并行）
    ///
    /// 核心优化：不遍历全量 N 个音符，只遍历 BitSet 中被置 1 的位。
    /// 对每个 chunk，找到其覆盖的 BitSet block 范围，用 trailing_zeros
    /// 定位被选中的位，再映射到 chunk 局部索引。
    ///
    /// 16M 50% 选中：~18ms（release mode, 8 线程）
    /// 16M 1% 选中：~0.4ms（trailing_zeros 跳过 99% 非选中位）
    /// 对比旧实现（for i in 0..N { if sel.get(i) }）：快 10-100x
    pub fn batch_move_parallel(
        &mut self,
        selected: &BitSet,
        delta_tick: f32,
        delta_key: i16,
        max_key: u16,
    ) -> usize {
        if self.total_len == 0 {
            return 0;
        }
        let num_threads = 8usize;
        let chunk_count = self.chunks.len();
        let chunks_per_thread = chunk_count.div_ceil(num_threads).max(1);
        let offsets = self.chunk_offsets.clone();
        let delta_key_val = delta_key as i32;
        let max_key_val = max_key as i32;

        std::thread::scope(|s| {
            for (thread_idx, chunk_group) in self.chunks.chunks_mut(chunks_per_thread).enumerate() {
                let group_start = thread_idx * chunks_per_thread;
                let offsets_ref = &offsets;
                let sel = selected;
                s.spawn(move || {
                    for (local_ci, chunk) in chunk_group.iter_mut().enumerate() {
                        let chunk_start = offsets_ref[group_start + local_ci];
                        if chunk.len == 0 {
                            continue;
                        }
                        let chunk_end = chunk_start + chunk.len;
                        let params = MoveChunkParams::new(
                            chunk_start,
                            chunk_end,
                            delta_tick,
                            delta_key_val,
                            max_key_val,
                        );
                        process_move_chunk_64(chunk, sel, &params);
                    }
                });
            }
        });

        selected.count_ones().min(self.total_len)
    }

    /// 从 `BitVec` 直接批量移动选中音符（消除 BitVec→BitSet 转换）
    ///
    /// 与 `batch_move_parallel` 等价，但接受 `&BitVec` 而非 `&BitSet`。
    /// 内部通过 `BitVec::blocks()` 获取 u32 块（bit-vec 0.8 默认），
    /// 再 trailing_zeros 遍历选中位。
    ///
    /// **注意**：bit-vec 0.8 的 `blocks()` 返回 u32 块（每块 32 位），
    /// 索引计算必须用 `*32` 而非 `*64`，否则选中位全部错位。
    ///
    /// 16M 50% 选中：~18ms（8 线程，与 batch_move_parallel 等价）
    pub fn batch_move_parallel_from_bitvec(
        &mut self,
        selected: &BitVec,
        delta_tick: f32,
        delta_key: i16,
        max_key: u16,
    ) -> usize {
        if self.total_len == 0 {
            return 0;
        }
        let num_threads = 8usize;
        let chunk_count = self.chunks.len();
        let chunks_per_thread = chunk_count.div_ceil(num_threads).max(1);
        let offsets = self.chunk_offsets.clone();
        let delta_key_val = delta_key as i32;
        let max_key_val = max_key as i32;

        // 收集 BitVec 的 u32 块到本地 Vec，支持随机访问
        // bit-vec 0.8 默认 Block = u32，每块 32 位
        let blocks: Vec<u32> = selected.blocks().collect();

        std::thread::scope(|s| {
            for (thread_idx, chunk_group) in self.chunks.chunks_mut(chunks_per_thread).enumerate() {
                let group_start = thread_idx * chunks_per_thread;
                let offsets_ref = &offsets;
                let blocks_ref = &blocks;
                s.spawn(move || {
                    for (local_ci, chunk) in chunk_group.iter_mut().enumerate() {
                        let chunk_start = offsets_ref[group_start + local_ci];
                        if chunk.len == 0 {
                            continue;
                        }
                        let chunk_end = chunk_start + chunk.len;
                        // bit-vec 0.8 默认 Block = u32，每块 32 位
                        let params = MoveChunkParams::new_32(
                            chunk_start,
                            chunk_end,
                            delta_tick,
                            delta_key_val,
                            max_key_val,
                        );
                        process_move_chunk_32(chunk, blocks_ref, &params);
                    }
                });
            }
        });

        selected.iter().filter(|&b| b).count()
    }
}

/// 处理单个 chunk 的移动参数包（将 7 个共享参数收口，避免 `too_many_arguments`）
#[derive(Clone, Copy)]
struct MoveChunkParams {
    chunk_start: usize,
    chunk_end: usize,
    start_block: usize,
    end_block: usize,
    delta_tick: f32,
    delta_key_val: i32,
    max_key_val: i32,
}

impl MoveChunkParams {
    fn new(
        chunk_start: usize,
        chunk_end: usize,
        delta_tick: f32,
        delta_key_val: i32,
        max_key_val: i32,
    ) -> Self {
        Self {
            chunk_start,
            chunk_end,
            start_block: chunk_start / 64,
            end_block: (chunk_end - 1) / 64,
            delta_tick,
            delta_key_val,
            max_key_val,
        }
    }

    fn new_32(
        chunk_start: usize,
        chunk_end: usize,
        delta_tick: f32,
        delta_key_val: i32,
        max_key_val: i32,
    ) -> Self {
        Self {
            chunk_start,
            chunk_end,
            start_block: chunk_start / 32,
            end_block: (chunk_end - 1) / 32,
            delta_tick,
            delta_key_val,
            max_key_val,
        }
    }
}

/// 处理单个 chunk 的 64-bit 块遍历移动（BitSet 路径）
#[inline]
fn process_move_chunk_64(chunk: &mut Chunk, sel: &BitSet, params: &MoveChunkParams) {
    for bi in params.start_block..=params.end_block {
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
            if global_idx >= params.chunk_start && global_idx < params.chunk_end {
                let local = global_idx - params.chunk_start;
                let new_tick = (chunk.ticks[local] + params.delta_tick).max(0.0);
                let new_key = (chunk.keys[local] as i32 + params.delta_key_val)
                    .clamp(0, params.max_key_val) as u16;
                if (chunk.ticks[local] - new_tick).abs() > f32::EPSILON
                    || chunk.keys[local] != new_key
                {
                    chunk.ticks[local] = new_tick;
                    chunk.keys[local] = new_key;
                }
            }
        }
    }
}

/// 处理单个 chunk 的 32-bit 块遍历移动（BitVec 路径）
#[inline]
fn process_move_chunk_32(chunk: &mut Chunk, blocks: &[u32], params: &MoveChunkParams) {
    for bi in params.start_block..=params.end_block {
        if bi >= blocks.len() {
            break;
        }
        let block = blocks[bi];
        if block == 0 {
            continue;
        }
        let base = bi * 32;
        let mut bits = block;
        while bits != 0 {
            let trailing_zeros_count = bits.trailing_zeros() as usize;
            let global_idx = base + trailing_zeros_count;
            bits &= bits - 1;
            if global_idx >= params.chunk_start && global_idx < params.chunk_end {
                let local = global_idx - params.chunk_start;
                let new_tick = (chunk.ticks[local] + params.delta_tick).max(0.0);
                let new_key = (chunk.keys[local] as i32 + params.delta_key_val)
                    .clamp(0, params.max_key_val) as u16;
                if (chunk.ticks[local] - new_tick).abs() > f32::EPSILON
                    || chunk.keys[local] != new_key
                {
                    chunk.ticks[local] = new_tick;
                    chunk.keys[local] = new_key;
                }
            }
        }
    }
}
