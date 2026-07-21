//! 架构: Segmented SoA (分块存储) 极致优化
//!
//! 核心思想: 将音符拆分为 SoA 布局的固定大小块 (chunk)
//!   - 每块 4096 音符 (48KB), 完美适配 L2 缓存
//!   - 插入只影响一个块, 不触发全量 realloc
//!   - 批量操作以块为单位并行, 无锁竞争
//!   - 无需预分配 slack, 天生支持增量增长
//!
//! 运行: cargo test --release -p lumino-core --test architecture_segmented_bench -- --nocapture

#![allow(clippy::unwrap_used)]

use std::time::Instant;

// ═══════════════════════════════════════════════════
// BitSet: Vec<u64> 基础
// ═══════════════════════════════════════════════════

#[derive(Clone)]
struct BitSet {
    blocks: Vec<u64>,
    len: usize,
}

impl BitSet {
    fn new(len: usize) -> Self {
        Self {
            blocks: vec![0; len.div_ceil(64)],
            len,
        }
    }

    fn from_fn<F: Fn(usize) -> bool>(len: usize, f: F) -> Self {
        let mut s = Self::new(len);
        for i in 0..len {
            if f(i) {
                s.set(i);
            }
        }
        s
    }

    fn set(&mut self, i: usize) {
        self.blocks[i / 64] |= 1 << (i % 64);
    }
    fn get(&self, i: usize) -> bool {
        (self.blocks[i / 64] >> (i % 64)) & 1 == 1
    }
    fn blocks(&self) -> &[u64] {
        &self.blocks
    }
    fn blocks_mut(&mut self) -> &mut [u64] {
        &mut self.blocks
    }
    fn block_count(&self) -> usize {
        self.blocks.len()
    }
    fn clone_blocks(&self) -> Vec<u64> {
        self.blocks.clone()
    }
    fn count_ones(&self) -> usize {
        self.blocks.iter().map(|b| b.count_ones() as usize).sum()
    }
    fn len(&self) -> usize {
        self.len
    }
}

fn sel_50pct(count: usize) -> BitSet {
    BitSet::from_fn(count, |i| i % 2 == 0)
}

fn sel_full(count: usize) -> BitSet {
    let mut s = BitSet::new(count);
    for b in s.blocks.iter_mut() {
        *b = !0;
    }
    let rem = count % 64;
    if rem > 0 {
        let last = s.blocks.len() - 1;
        s.blocks[last] &= (1u64 << rem) - 1;
    }
    s
}

// ═══════════════════════════════════════════════════
// Chunk: 48KB 的 SoA 音符块
// ═══════════════════════════════════════════════════

const CHUNK_SIZE: usize = 4096; // 48KB / chunk

struct Chunk {
    ticks: Vec<f32>,     // 4 bytes/note
    keys: Vec<u16>,      // 2 bytes/note
    lengths: Vec<f32>,   // 4 bytes/note
    velocities: Vec<u8>, // 1 byte/note
    channels: Vec<u8>,   // 1 byte/note
    len: usize,          // 实际音符数 (≤ CHUNK_SIZE)
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

    fn push(&mut self, tick: f32, key: u16, length: f32, velocity: u8, channel: u8) {
        self.ticks.push(tick);
        self.keys.push(key);
        self.lengths.push(length);
        self.velocities.push(velocity);
        self.channels.push(channel);
        self.len += 1;
    }

    fn truncate(&mut self, new_len: usize) {
        self.ticks.truncate(new_len);
        self.keys.truncate(new_len);
        self.lengths.truncate(new_len);
        self.velocities.truncate(new_len);
        self.channels.truncate(new_len);
        self.len = new_len;
    }

    fn capacity(&self) -> usize {
        CHUNK_SIZE
    }
    fn remaining(&self) -> usize {
        CHUNK_SIZE - self.len
    }
    fn is_full(&self) -> bool {
        self.len >= CHUNK_SIZE
    }
    fn data_bytes(&self) -> usize {
        self.ticks.capacity() * 4
            + self.keys.capacity() * 2
            + self.lengths.capacity() * 4
            + self.velocities.capacity() * 1
            + self.channels.capacity() * 1
    }
}

// ═══════════════════════════════════════════════════
// Segmented SoA NoteStore
// ═══════════════════════════════════════════════════

struct SegmentedNoteStore {
    chunks: Vec<Chunk>,
    chunk_offsets: Vec<usize>, // 前缀和: chunk_offsets[i] = 前 i 个 chunk 的总音符数
    tombstone: BitSet,
    selection: BitSet,
    total_len: usize,
}

impl SegmentedNoteStore {
    fn new(note_count: usize) -> Self {
        let chunk_count = note_count.div_ceil(CHUNK_SIZE);
        let mut chunks = Vec::with_capacity(chunk_count);
        let mut chunk_offsets = Vec::with_capacity(chunk_count + 1);
        chunk_offsets.push(0);

        for ci in 0..chunk_count {
            let start = ci * CHUNK_SIZE;
            let end = (start + CHUNK_SIZE).min(note_count);
            let count = end - start;
            let mut chunk = Chunk::new();
            for i in start..end {
                chunk.push(i as f32 * 10.0, 60 + (i % 24) as u16, 5.0, 100, 0);
            }
            chunk_offsets.push(note_count.min(chunk_offsets[ci] + count));
            chunks.push(chunk);
        }

        Self {
            chunks,
            chunk_offsets,
            tombstone: BitSet::new(note_count),
            selection: BitSet::new(note_count),
            total_len: note_count,
        }
    }

    /// 重建 chunk_offsets (插入/删除后调用)
    fn rebuild_offsets(&mut self) {
        self.chunk_offsets.clear();
        self.chunk_offsets.push(0);
        let mut acc = 0;
        for chunk in &self.chunks {
            acc += chunk.len;
            self.chunk_offsets.push(acc);
        }
    }

    /// 全局索引 → (chunk_idx, local_idx)  二分查找 O(log N)
    fn resolve(&self, global_idx: usize) -> (usize, usize) {
        let ci = self.chunk_offsets.partition_point(|&o| o <= global_idx) - 1;
        let local = global_idx - self.chunk_offsets[ci];
        (ci, local)
    }

    // ─── 批量移动: 块级并行 ──

    fn batch_move(&mut self, selected: &BitSet, delta_tick: f32, delta_key: i16) {
        let num_threads = 8;
        let chunk_count = self.chunks.len();
        if chunk_count == 0 {
            return;
        }
        let chunks_per_thread = chunk_count.div_ceil(num_threads);

        // 预计算每个 chunk 的全局起始索引 (共享不可变引用)
        let offsets = &self.chunk_offsets;

        std::thread::scope(|s| {
            for (thread_idx, chunk_group) in self.chunks.chunks_mut(chunks_per_thread).enumerate() {
                let group_start = thread_idx * chunks_per_thread;
                let sel = &selected;
                s.spawn(move || {
                    for (local_i, chunk) in chunk_group.iter_mut().enumerate() {
                        let chunk_global_start = offsets[group_start + local_i];
                        let chunk_len = chunk.len;
                        for i in 0..chunk_len {
                            let gi = chunk_global_start + i;
                            if gi < sel.len() && sel.get(gi) {
                                chunk.ticks[i] = (chunk.ticks[i] + delta_tick).max(0.0);
                                chunk.keys[i] =
                                    (chunk.keys[i] as i32 + delta_key as i32).clamp(0, 127) as u16;
                            }
                        }
                    }
                });
            }
        });
    }

    fn undo_move(&mut self, selected: &BitSet, delta_tick: f32, delta_key: i16) {
        self.batch_move(selected, -delta_tick, -delta_key);
    }

    // ─── 墓碑删除 (bulk OR) ──

    fn delete_selected(&mut self, selected: &BitSet) -> Vec<u64> {
        let undo = self.tombstone.clone_blocks();
        for (t, s) in self
            .tombstone
            .blocks_mut()
            .iter_mut()
            .zip(selected.blocks().iter())
        {
            *t |= s;
        }
        undo
    }

    fn undo_delete(&mut self, saved: Vec<u64>) {
        self.tombstone.blocks = saved;
    }

    // ─── 插入: 追加到末尾 (天生无预分配问题) ──

    fn insert_notes(&mut self, count: usize) -> usize {
        let start = self.total_len;
        if count == 0 {
            return start;
        }

        if let Some(last) = self.chunks.last_mut() {
            let room = last.remaining();
            if room >= count {
                // 最后一个 chunk 有足够空间
                for i in 0..count {
                    last.push((start + i) as f32 * 10.0, 60, 5.0, 100, 0);
                }
            } else {
                // 填满最后一个 chunk
                for _ in 0..room {
                    last.push(0.0, 60, 5.0, 100, 0);
                }
                let remaining = count - room;
                // 创建新 chunk(s), 每个最多 CHUNK_SIZE
                let mut remaining = remaining;
                while remaining > 0 {
                    let batch = remaining.min(CHUNK_SIZE);
                    let mut new_chunk = Chunk::new();
                    for i in 0..batch {
                        new_chunk.push(
                            (start + count - remaining + i) as f32 * 10.0,
                            60,
                            5.0,
                            100,
                            0,
                        );
                    }
                    self.chunks.push(new_chunk);
                    remaining -= batch;
                }
            }
        } else {
            // 空 store
            let mut remaining = count;
            while remaining > 0 {
                let batch = remaining.min(CHUNK_SIZE);
                let mut chunk = Chunk::new();
                for i in 0..batch {
                    chunk.push((count - remaining + i) as f32 * 10.0, 60, 5.0, 100, 0);
                }
                self.chunks.push(chunk);
                remaining -= batch;
            }
        }

        self.total_len += count;
        self.tombstone.blocks.resize(self.total_len.div_ceil(64), 0);
        self.rebuild_offsets();
        start
    }

    fn undo_insert(&mut self, start_idx: usize) {
        let to_remove = self.total_len - start_idx;
        let mut remaining = to_remove;
        while remaining > 0 {
            if let Some(last) = self.chunks.last_mut() {
                if last.len <= remaining {
                    remaining -= last.len;
                    self.chunks.pop();
                } else {
                    let new_len = last.len - remaining;
                    last.truncate(new_len);
                    remaining = 0;
                }
            } else {
                break;
            }
        }
        self.total_len = start_idx;
        self.tombstone.blocks.resize(start_idx.div_ceil(64), 0);
        self.rebuild_offsets();
    }

    fn len(&self) -> usize {
        self.total_len
    }

    fn memory_mb(&self) -> f64 {
        let mut vec_cap: usize = 0;
        for chunk in &self.chunks {
            vec_cap += chunk.ticks.capacity() * 4
                + chunk.keys.capacity() * 2
                + chunk.lengths.capacity() * 4
                + chunk.velocities.capacity() * 1
                + chunk.channels.capacity() * 1;
        }
        let bitset = self.tombstone.blocks.capacity() * 8 * 2;
        let offsets = self.chunk_offsets.capacity() * std::mem::size_of::<usize>();
        let chunks_vec = self.chunks.capacity() * std::mem::size_of::<Chunk>();
        (vec_cap + bitset + offsets + chunks_vec) as f64 / (1024.0 * 1024.0)
    }

    fn active_count(&self) -> usize {
        self.total_len - self.tombstone.count_ones()
    }
}

// ═══════════════════════════════════════════════════
// AoS (v3) 对比基准
// ═══════════════════════════════════════════════════

#[derive(Clone)]
#[repr(C)]
struct Note {
    tick: f32,
    key: u16,
    length: f32,
    velocity: u8,
    channel: u8,
}

struct AoSNoteStore {
    notes: Vec<Note>,
    tombstone: BitSet,
    total_len: usize,
}

impl AoSNoteStore {
    fn new(note_count: usize) -> Self {
        let slack = note_count / 10;
        let mut notes = Vec::with_capacity(note_count + slack);
        for i in 0..note_count {
            notes.push(Note {
                tick: i as f32 * 10.0,
                key: 60 + (i % 24) as u16,
                length: 5.0,
                velocity: 100,
                channel: 0,
            });
        }
        Self {
            notes,
            tombstone: BitSet::new(note_count),
            total_len: note_count,
        }
    }

    /// 无 slack 版本 (用于公平比较无预分配插入)
    fn new_no_slack(note_count: usize) -> Self {
        let mut notes = Vec::with_capacity(note_count); // 精确容量, 无 slack
        for i in 0..note_count {
            notes.push(Note {
                tick: i as f32 * 10.0,
                key: 60 + (i % 24) as u16,
                length: 5.0,
                velocity: 100,
                channel: 0,
            });
        }
        Self {
            notes,
            tombstone: BitSet::new(note_count),
            total_len: note_count,
        }
    }

    fn batch_move(&mut self, selected: &BitSet, delta_tick: f32, delta_key: i16) {
        let num_blocks = selected.block_count();
        let block_chunk_size = num_blocks.div_ceil(8);
        let note_chunk_size = block_chunk_size * 64;

        std::thread::scope(|s| {
            for (thread_idx, chunk) in self.notes.chunks_mut(note_chunk_size).enumerate() {
                let block_start = thread_idx * block_chunk_size;
                let block_end = (block_start + block_chunk_size).min(num_blocks);
                let sel = &selected;
                s.spawn(move || {
                    for (local_bi, &block) in
                        sel.blocks()[block_start..block_end].iter().enumerate()
                    {
                        let base = local_bi * 64;
                        let mut bits = block;
                        while bits != 0 {
                            let tz = bits.trailing_zeros();
                            let idx = base + tz as usize;
                            if idx < chunk.len() {
                                chunk[idx].tick = (chunk[idx].tick + delta_tick).max(0.0);
                                chunk[idx].key =
                                    (chunk[idx].key as i32 + delta_key as i32).clamp(0, 127) as u16;
                            }
                            bits &= bits - 1;
                        }
                    }
                });
            }
        });
    }

    fn delete_selected(&mut self, selected: &BitSet) -> Vec<u64> {
        let undo = self.tombstone.clone_blocks();
        for (t, s) in self
            .tombstone
            .blocks_mut()
            .iter_mut()
            .zip(selected.blocks().iter())
        {
            *t |= s;
        }
        undo
    }

    fn undo_delete(&mut self, saved: Vec<u64>) {
        self.tombstone.blocks = saved;
    }

    fn insert_notes(&mut self, count: usize) -> usize {
        let start = self.notes.len();
        // 无预分配: 先 shrink, 再 push, 触发 realloc
        self.notes.shrink_to_fit();
        for _ in 0..count {
            self.notes.push(Note {
                tick: 0.0,
                key: 60,
                length: 5.0,
                velocity: 100,
                channel: 0,
            });
        }
        self.tombstone
            .blocks
            .resize(self.notes.len().div_ceil(64), 0);
        self.total_len = self.notes.len();
        start
    }

    fn undo_insert(&mut self, start: usize) {
        self.notes.truncate(start);
        self.tombstone.blocks.resize(start.div_ceil(64), 0);
        self.total_len = start;
    }

    fn len(&self) -> usize {
        self.total_len
    }

    fn memory_mb(&self) -> f64 {
        let data = self.notes.capacity() * std::mem::size_of::<Note>();
        let bitset = self.tombstone.blocks.capacity() * 8;
        (data + bitset) as f64 / (1024.0 * 1024.0)
    }
}

// ═══════════════════════════════════════════════════
// 测试 1: 内存对比
// ═══════════════════════════════════════════════════

#[test]
fn bench_memory_comparison() {
    let count: usize = 16_000_000;

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!("  内存开销对比: {} 音符", count);
    eprintln!("═══════════════════════════════════════════════════");

    // 理论值
    let aos_per_note: usize = 16; // bytes
    let soa_per_note: usize = 12; // bytes
    let seg_per_note: usize = 12; // bytes (SoA 数据)

    let aos_data = count * aos_per_note;
    let soa_data = count * soa_per_note;
    let seg_data = count * seg_per_note;
    let bitset = count / 8;

    let seg_chunks = count.div_ceil(CHUNK_SIZE);
    let seg_overhead = seg_chunks * std::mem::size_of::<Chunk>()   // Chunk struct
        + seg_chunks * 5 * 3 * std::mem::size_of::<usize>(); // Vec 3 pointers × 5 fields

    eprintln!();
    eprintln!("  类别               |  AoS (v3)  |  SoA (flat) |  Segmented");
    eprintln!("  ────────────────────────────────────────────────────────────");
    eprintln!(
        "  数据 (16M)         |  {:>6} MB  |  {:>6} MB  |  {:>6} MB",
        aos_data / (1024 * 1024),
        soa_data / (1024 * 1024),
        seg_data / (1024 * 1024)
    );
    eprintln!(
        "  BitSet ×2          |  {:>6} MB  |  {:>6} MB  |  {:>6} MB",
        bitset * 2 / (1024 * 1024),
        bitset * 2 / (1024 * 1024),
        bitset * 2 / (1024 * 1024)
    );
    eprintln!(
        "  Slack 10%          |  {:>6} MB  |  {:>6} MB  |  {:>6} MB",
        (count / 10) * 16 / (1024 * 1024),
        (count / 10) * 12 / (1024 * 1024),
        0
    );
    eprintln!(
        "  块级 Vec 开销      |  {:>6} MB  |  {:>6} MB  |  {:>6} MB",
        0,
        0,
        seg_overhead / (1024 * 1024)
    );

    let aos_total = aos_data + bitset * 2 + (count / 10) * 16;
    let soa_total = soa_data + bitset * 2 + (count / 10) * 12;
    let seg_total = seg_data + bitset * 2 + seg_overhead;
    eprintln!("  ────────────────────────────────────────────────────────────");
    eprintln!(
        "  合计               |  {:>6} MB  |  {:>6} MB  |  {:>6} MB",
        aos_total / (1024 * 1024),
        soa_total / (1024 * 1024),
        seg_total / (1024 * 1024)
    );
    eprintln!(
        "  相对 AoS 节省      |  {:>6} MB  |  {:>6} MB  |  {:>6} MB",
        0,
        (aos_total - soa_total) / (1024 * 1024),
        (aos_total - seg_total) / (1024 * 1024)
    );
    eprintln!();

    // 100M
    eprintln!("  ── 100M 外推 ──");
    let _aos_100m =
        (100_000_000 * 16 + (100_000_000 / 8) * 2 + (100_000_000 / 10) * 16) / (1024 * 1024);
    let _soa_100m =
        (100_000_000 * 12 + (100_000_000 / 8) * 2 + (100_000_000 / 10) * 12) / (1024 * 1024);
    let seg_100m_chunks = 100_000_000usize.div_ceil(CHUNK_SIZE);
    let seg_100m_overhead = seg_100m_chunks * std::mem::size_of::<Chunk>()
        + seg_100m_chunks * 5 * 3 * std::mem::size_of::<usize>();
    let _seg_100m = (100_000_000 * 12 + (100_000_000 / 8) * 2 + seg_100m_overhead) / (1024 * 1024);

    eprintln!("  AoS:     {} MB/100M  (含 10% slack)", _aos_100m);
    eprintln!("  SoA:     {} MB/100M  (含 10% slack)", _soa_100m);
    eprintln!("  Segmented: {} MB/100M  (无 slack 需求)", _seg_100m);

    // 实际测量
    eprintln!("\n  ── 实际测量 (16M) ──");
    let seg = SegmentedNoteStore::new(count);
    let aos = AoSNoteStore::new(count);
    eprintln!("  AoS:     {:.0} MB", aos.memory_mb());
    eprintln!("  Segmented: {:.0} MB", seg.memory_mb());
    drop(seg);
    drop(aos);
}

// ═══════════════════════════════════════════════════
// 测试 2: 插入速度 (无预分配)
// ═══════════════════════════════════════════════════

#[test]
fn bench_insert_no_prealloc() {
    let count = 16_000_000;
    let insert_counts = [10, 100, 1000, 10_000, 100_000];

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!("  插入速度 (无预分配): {} 基础音符", count);
    eprintln!("═══════════════════════════════════════════════════");
    eprintln!("  插入数  |  Segmented   |  AoS (v3)    |  加速比");

    for &ic in &insert_counts {
        // Segmented
        let mut seg = SegmentedNoteStore::new(count);
        let t = Instant::now();
        let start = seg.insert_notes(ic);
        seg.undo_insert(start);
        let seg_time = t.elapsed();

        // AoS (无预分配, 无 slack)
        let mut aos = AoSNoteStore::new_no_slack(count);
        let t = Instant::now();
        let start = aos.insert_notes(ic);
        aos.undo_insert(start);
        let aos_time = t.elapsed();

        let speedup = if seg_time < std::time::Duration::from_nanos(1) {
            f64::INFINITY
        } else {
            aos_time.as_secs_f64() / seg_time.as_secs_f64()
        };

        eprintln!(
            "  {:>7}  |  {:>10?}  |  {:>10?}  |  {:>5.0}x",
            ic, seg_time, aos_time, speedup
        );
    }
}

// ═══════════════════════════════════════════════════
// 测试 3: 批量移动 (50% 选中)
// ═══════════════════════════════════════════════════

#[test]
fn bench_batch_move_50pct() {
    let counts = [5_000_000, 10_000_000, 16_000_000];

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!("  批量移动 (50% 选中)");
    eprintln!("═══════════════════════════════════════════════════");
    eprintln!("  音符数    |  Segmented   |  AoS (v3)    |  加速比");

    for &count in &counts {
        let sel = sel_50pct(count);

        // Segmented
        let mut seg = SegmentedNoteStore::new(count);
        let t = Instant::now();
        seg.batch_move(&sel, 10.0, 3);
        let seg_time = t.elapsed();

        // AoS
        let mut aos = AoSNoteStore::new(count);
        let t = Instant::now();
        aos.batch_move(&sel, 10.0, 3);
        let aos_time = t.elapsed();

        let speedup = aos_time.as_secs_f64() / seg_time.as_secs_f64();
        let rate = (count as f64) / seg_time.as_secs_f64() / 1_000_000.0;
        eprintln!(
            "  {:>9} |  {:>10?}  |  {:>10?}  |  {:>5.1}x  ({:.0}M/s)",
            count, seg_time, aos_time, speedup, rate
        );
    }
}

// ═══════════════════════════════════════════════════
// 测试 4: 全选移动
// ═══════════════════════════════════════════════════

#[test]
fn bench_batch_move_full() {
    let counts = [5_000_000, 10_000_000, 16_000_000];

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!("  批量移动 (全选)");
    eprintln!("═══════════════════════════════════════════════════");
    eprintln!("  音符数    |  Segmented   |  AoS (v3)    |  加速比");

    for &count in &counts {
        let sel = sel_full(count);

        let mut seg = SegmentedNoteStore::new(count);
        let t = Instant::now();
        seg.batch_move(&sel, 10.0, 3);
        let seg_time = t.elapsed();

        let mut aos = AoSNoteStore::new(count);
        let t = Instant::now();
        aos.batch_move(&sel, 10.0, 3);
        let aos_time = t.elapsed();

        let speedup = aos_time.as_secs_f64() / seg_time.as_secs_f64();
        let rate = (count as f64) / seg_time.as_secs_f64() / 1_000_000.0;
        eprintln!(
            "  {:>9} |  {:>10?}  |  {:>10?}  |  {:>5.1}x  ({:.0}M/s)",
            count, seg_time, aos_time, speedup, rate
        );
    }
}

// ═══════════════════════════════════════════════════
// 测试 5: 完整工作流
// ═══════════════════════════════════════════════════

#[test]
fn bench_full_workflow() {
    let count = 16_000_000;
    let sel = sel_50pct(count);
    let mut seg = SegmentedNoteStore::new(count);

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!("  完整工作流 (Segmented): {} 音符", count);
    eprintln!("═══════════════════════════════════════════════════");
    let mut total = std::time::Duration::ZERO;

    // 1. 批量移动 50%
    let t = Instant::now();
    seg.batch_move(&sel, 10.0, 3);
    let e = t.elapsed();
    total += e;
    eprintln!("  [1/5] 批量移动 50%: {:?}", e);

    // 2. 撤销移动
    let t = Instant::now();
    seg.undo_move(&sel, 10.0, 3);
    let e = t.elapsed();
    total += e;
    eprintln!("  [2/5] 撤销移动: {:?}", e);

    // 3. 删除 50%
    let t = Instant::now();
    let saved = seg.delete_selected(&sel);
    let e = t.elapsed();
    total += e;
    eprintln!("  [3/5] 删除 50%: {:?}", e);
    eprintln!("         undo 内存: {} MB", saved.len() * 8 / (1024 * 1024));

    // 4. 撤销删除
    let t = Instant::now();
    seg.undo_delete(saved);
    let e = t.elapsed();
    total += e;
    eprintln!("  [4/5] 撤销删除: {:?}", e);

    // 5. 插入 1000 音符 (无预分配)
    let t = Instant::now();
    let start = seg.insert_notes(1000);
    seg.undo_insert(start);
    let e = t.elapsed();
    total += e;
    eprintln!("  [5/5] 插入 1000 (无预分配): {:?}", e);

    eprintln!("  ────────────────────────────────────────");
    eprintln!("  总耗时: {:?}", total);
    eprintln!("  内存:   {:.0} MB", seg.memory_mb());
    drop(seg);

    // AoS 对比 (不含 undo_move)
    let mut aos = AoSNoteStore::new_no_slack(count);
    let mut total_aos = std::time::Duration::ZERO;

    let t = Instant::now();
    aos.batch_move(&sel, 10.0, 3);
    let e = t.elapsed();
    total_aos += e;
    eprintln!("  [1/5] 批量移动 50%: {:?}", e);

    let t = Instant::now();
    let saved = aos.delete_selected(&sel);
    let e = t.elapsed();
    total_aos += e;
    eprintln!("  [3/5] 删除 50%: {:?}", e);

    let t = Instant::now();
    aos.undo_delete(saved);
    let e = t.elapsed();
    total_aos += e;
    eprintln!("  [4/5] 撤销删除: {:?}", e);

    // AoS 插入 (无预分配)
    let t = Instant::now();
    let start = aos.insert_notes(1000);
    aos.undo_insert(start);
    let e = t.elapsed();
    total_aos += e;
    eprintln!("  [5/5] 插入 1000 (无预分配): {:?}", e);

    eprintln!("  ────────────────────────────────────────");
    eprintln!("  AoS 总耗时: {:?}  (不含 undo_move)", total_aos);
    eprintln!("  AoS 内存:   {:.0} MB", aos.memory_mb());
}

// ═══════════════════════════════════════════════════
// 测试 6: 100M 外推
// ═══════════════════════════════════════════════════

#[test]
fn bench_100m_extrapolation() {
    let count = 16_000_000;
    let sel_50 = sel_50pct(count);
    let sel_full = sel_full(count);

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!("  100M 外推");
    eprintln!("═══════════════════════════════════════════════════");

    let seg_100m_data = (100_000_000 * 12) as f64 / (1024.0 * 1024.0);
    eprintln!("  Segmented 100M 数据: {:.0} MB", seg_100m_data);
    eprintln!(
        "  AoS 100M 数据:      {:.0} MB",
        (100_000_000 * 16) as f64 / (1024.0 * 1024.0)
    );
    eprintln!(
        "  100M undo 内存:     {} MB (BitVec, 可逆操作)",
        (100_000_000 / 8) / (1024 * 1024)
    );

    let factor = 100_000_000.0 / count as f64;

    // 50% 选中
    {
        let mut seg = SegmentedNoteStore::new(count);
        let t = Instant::now();
        seg.batch_move(&sel_50, 10.0, 3);
        let _16m = t.elapsed();
        eprintln!("\n  Segmented 16M 50%: {:?}", _16m);
        eprintln!(
            "  Segmented 100M 50%: {:?}",
            std::time::Duration::from_secs_f64(_16m.as_secs_f64() * factor)
        );

        // 预估 100M 实际 (考虑缓存失效, 加 30% 惩罚)
        let with_penalty = _16m.as_secs_f64() * factor * 1.3;
        eprintln!(
            "  100M 50% (含 30% 缓存惩罚): {:?}",
            std::time::Duration::from_secs_f64(with_penalty)
        );
    }

    // 全选
    {
        let mut seg = SegmentedNoteStore::new(count);
        let t = Instant::now();
        seg.batch_move(&sel_full, 10.0, 3);
        let _16m = t.elapsed();
        eprintln!("\n  Segmented 16M 100%: {:?}", _16m);
        eprintln!(
            "  Segmented 100M 100%: {:?}",
            std::time::Duration::from_secs_f64(_16m.as_secs_f64() * factor)
        );

        let with_penalty = _16m.as_secs_f64() * factor * 1.3;
        eprintln!(
            "  100M 100% (含 30% 缓存惩罚): {:?}",
            std::time::Duration::from_secs_f64(with_penalty)
        );
    }

    // 理论极限
    eprintln!("\n  ── 理论极限 (40GB/s DDR4) ──");
    let soa_ticks_mb = (100_000_000 * 4) as f64 / (1024.0 * 1024.0);
    let soa_keys_mb = (100_000_000 * 2) as f64 / (1024.0 * 1024.0);
    let soa_read = soa_ticks_mb + soa_keys_mb;
    let soa_write = soa_ticks_mb / 2.0 + soa_keys_mb / 2.0;
    let total_mb = soa_read + soa_write;
    let theoretical_ms = total_mb * 1024.0 * 1024.0 / (40.0 * 1024.0 * 1024.0 * 1024.0) * 1000.0;
    eprintln!(
        "  SoA 内存流量: {:.0} MB read + {:.0} MB write = {:.0} MB total",
        soa_read, soa_write, total_mb
    );
    eprintln!("  理论极限: {:.1} ms", theoretical_ms);
    eprintln!("  实际预期 (50% 效率): {:.1} ms", theoretical_ms * 2.0);

    // 插入 1000 (无预分配) 100M 外推
    {
        let mut seg = SegmentedNoteStore::new(count);
        let t = Instant::now();
        let start = seg.insert_notes(1000);
        seg.undo_insert(start);
        let _16m = t.elapsed();
        eprintln!("\n  Segmented 插入 1000 (无预分配): {:?}", _16m);
        // Segmented 插入不受总数据量影响, 100M 也是这个速度
        eprintln!("  100M 插入 1000 (无预分配): {:?} (不变)", _16m);
    }
}

// ═══════════════════════════════════════════════════
// 测试 7: 线程扩展性
// ═══════════════════════════════════════════════════

#[test]
fn bench_thread_scaling() {
    let count = 16_000_000;
    let sel = sel_50pct(count);

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!("  线程扩展性: {} 音符, 50% 选中", count);
    eprintln!("═══════════════════════════════════════════════════");
    eprintln!("  线程数 |  Segmented   |  AoS (v3)    |  吞吐量 (Seg)");

    for &threads in &[1, 2, 4, 8, 16] {
        // Segmented
        let mut seg = SegmentedNoteStore::new(count);
        let chunk_count = seg.chunks.len();
        let chunks_per_thread = chunk_count.div_ceil(threads);
        let offsets = &seg.chunk_offsets;

        let t = Instant::now();
        std::thread::scope(|s| {
            for (ti, group) in seg.chunks.chunks_mut(chunks_per_thread).enumerate() {
                let group_start = ti * chunks_per_thread;
                let sel = &sel;
                s.spawn(move || {
                    for (local_i, chunk) in group.iter_mut().enumerate() {
                        let gs = offsets[group_start + local_i];
                        for i in 0..chunk.len {
                            let gi = gs + i;
                            if gi < sel.len() && sel.get(gi) {
                                chunk.ticks[i] = (chunk.ticks[i] + 10.0).max(0.0);
                                chunk.keys[i] = (chunk.keys[i] as i32 + 3).clamp(0, 127) as u16;
                            }
                        }
                    }
                });
            }
        });
        let seg_time = t.elapsed();
        let rate = (count as f64) / seg_time.as_secs_f64() / 1_000_000.0;

        // AoS
        let mut aos = AoSNoteStore::new(count);
        let num_blocks = sel.block_count();
        let block_chunk_size = num_blocks.div_ceil(threads);
        let note_chunk_size = block_chunk_size * 64;

        let t = Instant::now();
        std::thread::scope(|s| {
            for (ti, chunk) in aos.notes.chunks_mut(note_chunk_size).enumerate() {
                let bs = ti * block_chunk_size;
                let be = (bs + block_chunk_size).min(num_blocks);
                let sel = &sel;
                s.spawn(move || {
                    for (lbi, &block) in sel.blocks()[bs..be].iter().enumerate() {
                        let base = lbi * 64;
                        let mut bits = block;
                        while bits != 0 {
                            let tz = bits.trailing_zeros();
                            let idx = base + tz as usize;
                            if idx < chunk.len() {
                                chunk[idx].tick += 10.0;
                                chunk[idx].key = (chunk[idx].key as i32 + 3).clamp(0, 127) as u16;
                            }
                            bits &= bits - 1;
                        }
                    }
                });
            }
        });
        let aos_time = t.elapsed();

        eprintln!(
            "  {:>3}    |  {:>10?} |  {:>10?} |  {:.0}M/s",
            threads, seg_time, aos_time, rate
        );
    }
}

// ═══════════════════════════════════════════════════
// 测试 8: 大批量插入 + 删除 + 压缩
// ═══════════════════════════════════════════════════

#[test]
fn bench_large_insert_and_compact() {
    let count = 16_000_000;
    let mut seg = SegmentedNoteStore::new(count);

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!("  大批量操作: {} 基础音符", count);
    eprintln!("═══════════════════════════════════════════════════");

    // 100K 插入 (无预分配)
    let t = Instant::now();
    let start = seg.insert_notes(100_000);
    let insert_time = t.elapsed();
    eprintln!("  插入 100K (无预分配): {:?}", insert_time);
    eprintln!(
        "  插入后 chunk 数: {}, 总音符: {}",
        seg.chunks.len(),
        seg.len()
    );

    // 撤销
    let t = Instant::now();
    seg.undo_insert(start);
    eprintln!("  撤销插入 100K: {:?}", t.elapsed());

    // 1M 插入 (无预分配)
    let t = Instant::now();
    let start = seg.insert_notes(1_000_000);
    let insert_1m = t.elapsed();
    eprintln!("\n  插入 1M (无预分配): {:?}", insert_1m);
    eprintln!(
        "  插入后 chunk 数: {}, 总音符: {}",
        seg.chunks.len(),
        seg.len()
    );
    eprintln!("  内存: {:.0} MB", seg.memory_mb());

    // 删除 50% + 压缩
    let sel_50 = sel_50pct(seg.len());
    let t = Instant::now();
    let saved = seg.delete_selected(&sel_50);
    let del_time = t.elapsed();

    let t = Instant::now();
    // 压缩: 重建 chunks, 只保留未删除的音符
    let mut new_chunks: Vec<Chunk> = Vec::new();
    let mut offsets = Vec::new();
    offsets.push(0);
    let mut global_idx = 0usize;
    for chunk in &seg.chunks {
        let mut new_chunk = Chunk::new();
        for i in 0..chunk.len {
            if !seg.tombstone.get(global_idx + i) {
                new_chunk.push(
                    chunk.ticks[i],
                    chunk.keys[i],
                    chunk.lengths[i],
                    chunk.velocities[i],
                    chunk.channels[i],
                );
            }
        }
        if new_chunk.len > 0 {
            offsets.push(new_chunk.len);
            new_chunks.push(new_chunk);
        }
        global_idx += chunk.len;
    }
    let compact_time = t.elapsed();

    eprintln!("  删除 50%: {:?}", del_time);
    eprintln!(
        "  压缩: {:?}, 压缩后 {} 音符, {} chunks",
        compact_time,
        new_chunks.iter().map(|c| c.len).sum::<usize>(),
        new_chunks.len()
    );

    // 恢复
    seg.undo_delete(saved);
    seg.undo_insert(start);

    eprintln!(
        "\n  最终恢复: {} 音符, {} chunks",
        seg.len(),
        seg.chunks.len()
    );
}

// ═══════════════════════════════════════════════════
// 测试 9: 稀疏选择 (1% 选中)
// ═══════════════════════════════════════════════════

#[test]
fn bench_sparse_selection() {
    let count = 16_000_000;
    let sel_sparse = BitSet::from_fn(count, |i| i % 100 == 0);

    eprintln!("\n═══════════════════════════════════════════════════");
    eprintln!(
        "  稀疏选择: {} 音符, 1% 选中 ({})",
        count,
        sel_sparse.count_ones()
    );
    eprintln!("═══════════════════════════════════════════════════");

    // Segmented
    let mut seg = SegmentedNoteStore::new(count);
    let t = Instant::now();
    seg.batch_move(&sel_sparse, 10.0, 3);
    eprintln!(
        "  Segmented: {:?} (修改 {} 音符)",
        t.elapsed(),
        sel_sparse.count_ones()
    );

    // AoS
    let mut aos = AoSNoteStore::new(count);
    let t = Instant::now();
    aos.batch_move(&sel_sparse, 10.0, 3);
    eprintln!("  AoS:       {:?}", t.elapsed());
}
