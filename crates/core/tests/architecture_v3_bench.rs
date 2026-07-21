//! 新架构 v3: 极致优化版 release mode 基准测试
//!
//! 核心优化: BitSet (Vec<u64>) 替代 bit-vec, 只遍历选中位, 无浪费循环
//! 运行: cargo test --release -p lumino-core --test architecture_v3_bench -- --nocapture

use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
#[repr(C)]
struct Note {
    tick: f32,
    key: u16,
    length: f32,
    velocity: u8,
    channel: u8,
}

impl Note {
    fn new(tick: f32, key: u16, length: f32) -> Self {
        Self { tick, key, length, velocity: 100, channel: 0 }
    }
}

fn generate_notes(count: usize) -> Vec<Note> {
    (0..count).map(|i| Note::new(i as f32 * 10.0, 60 + (i % 24) as u16, 5.0)).collect()
}

// ─── BitSet: Vec<u64> 基础, 支持 block 级操作 ───────────────

#[derive(Clone, Debug)]
struct BitSet {
    blocks: Vec<u64>,
    len: usize,
}

impl BitSet {
    fn new(len: usize) -> Self {
        let block_count = (len + 63) / 64;
        Self { blocks: vec![0; block_count], len }
    }

    fn from_fn<F: Fn(usize) -> bool>(len: usize, f: F) -> Self {
        let mut set = Self::new(len);
        for i in 0..len {
            if f(i) {
                set.set(i);
            }
        }
        set
    }

    fn set(&mut self, index: usize) {
        self.blocks[index / 64] |= 1 << (index % 64);
    }

    fn get(&self, index: usize) -> bool {
        (self.blocks[index / 64] >> (index % 64)) & 1 == 1
    }

    fn blocks(&self) -> &[u64] { &self.blocks }
    fn blocks_mut(&mut self) -> &mut [u64] { &mut self.blocks }
    fn block_count(&self) -> usize { self.blocks.len() }

    /// 克隆 blocks (用于 undo 保存)
    fn clone_blocks(&self) -> Vec<u64> {
        self.blocks.clone()
    }

    /// 只遍历被设置的位（核心优化！）
    /// 对每个被设置的位调用 callback
    fn for_each_set_bit(&self, block_start: usize, block_end: usize, mut f: impl FnMut(usize)) {
        let bits_slice = &self.blocks[block_start..block_end];
        for (i, &block) in bits_slice.iter().enumerate() {
            let base = (block_start + i) * 64;
            let mut b = block;
            while b != 0 {
                let tz = b.trailing_zeros();
                f(base + tz as usize);
                b &= b - 1; // 清除最低位
            }
        }
    }

    /// 统计被设置的位数
    fn count_ones(&self) -> usize {
        self.blocks.iter().map(|b| b.count_ones() as usize).sum()
    }

    fn len(&self) -> usize { self.len }
}

fn generate_selection_50pct(count: usize) -> BitSet {
    BitSet::from_fn(count, |i| i % 2 == 0)
}

fn generate_selection_full(count: usize) -> BitSet {
    let mut set = BitSet::new(count);
    for b in set.blocks.iter_mut() {
        *b = !0; // 全 1
    }
    // 清除超出范围的位
    let remainder = count % 64;
    if remainder > 0 {
        let last = set.blocks.len() - 1;
        set.blocks[last] &= (1 << remainder) - 1;
    }
    set
}

// ─── 极致优化版 NoteStore ─────────────────────────────────

struct NoteStore {
    notes: Vec<Note>,
    tombstone: BitSet,  // 1 = 已删除

    // 预分配 slack, 避免 insert 触发 realloc
    slack: usize,
}

impl NoteStore {
    fn new(notes: Vec<Note>) -> Self {
        let len = notes.len();
        // 预分配 10% slack
        let slack = (len / 10).max(1024);
        let mut store = Self {
            notes,
            tombstone: BitSet::new(len),
            slack,
        };
        store.notes.reserve(slack);
        store
    }

    // ─── 批量移动（核心优化：block-based, 只遍历选中的位） ──

    fn batch_move(&mut self, selected: &BitSet, delta_tick: f32, delta_key: i16) {
        let num_blocks = selected.block_count();
        let num_threads = 8;
        let block_chunk_size = num_blocks.div_ceil(num_threads);
        let note_chunk_size = block_chunk_size * 64;

        std::thread::scope(|s| {
            for (thread_idx, note_chunk) in self.notes.chunks_mut(note_chunk_size).enumerate() {
                let block_start = thread_idx * block_chunk_size;
                let block_end = (block_start + block_chunk_size).min(num_blocks);

                s.spawn(move || {
                    // 只遍历选中的位，跳过所有非选中位
                    for (local_bi, &block) in selected.blocks()[block_start..block_end].iter().enumerate() {
                        let base = local_bi * 64;
                        let mut bits = block;
                        while bits != 0 {
                            let tz = bits.trailing_zeros();
                            let idx = base + tz as usize;
                            if idx < note_chunk.len() {
                                note_chunk[idx].tick = (note_chunk[idx].tick + delta_tick).max(0.0);
                                note_chunk[idx].key = (note_chunk[idx].key as i32 + delta_key as i32).clamp(0, 127) as u16;
                            }
                            bits &= bits - 1;
                        }
                    }
                });
            }
        });
    }

    /// 撤销移动（反向操作）
    fn undo_move(&mut self, selected: &BitSet, delta_tick: f32, delta_key: i16) {
        self.batch_move(selected, -delta_tick, -delta_key);
    }

    // ─── 墓碑删除（核心优化：bulk OR, 无逐位循环） ──

    fn delete_selected(&mut self, selected: &BitSet) -> Vec<u64> {
        let undo = self.tombstone.clone_blocks();
        // 批量 OR: 一次操作 64 位
        for (t, s) in self.tombstone.blocks_mut().iter_mut().zip(selected.blocks().iter()) {
            *t |= s;
        }
        undo
    }

    fn undo_delete(&mut self, saved_blocks: Vec<u64>) {
        self.tombstone.blocks = saved_blocks;
    }

    // ─── 插入（核心优化：预分配避免 realloc） ──

    fn insert_notes(&mut self, new_notes: &[Note]) -> (usize, Vec<Note>) {
        let start_idx = self.notes.len();
        // 确保有足够容量
        let needed = self.notes.len() + new_notes.len();
        if self.notes.capacity() < needed {
            let new_cap = (needed * 11) / 10 + self.slack;
            self.notes.reserve(new_cap - self.notes.len());
        }
        self.notes.extend_from_slice(new_notes);
        self.tombstone.blocks.resize(self.notes.len().div_ceil(64), 0);
        (start_idx, new_notes.to_vec())
    }

    fn undo_insert(&mut self, start_idx: usize) {
        self.notes.truncate(start_idx);
        let new_block_count = start_idx.div_ceil(64);
        self.tombstone.blocks.truncate(new_block_count);
    }

    // ─── 单音符修改 ──

    fn modify_note(&mut self, index: usize, new_tick: f32, new_key: u16, new_len: f32) -> Note {
        let old = self.notes[index].clone();
        self.notes[index].tick = new_tick;
        self.notes[index].key = new_key;
        self.notes[index].length = new_len;
        old
    }

    fn undo_modify(&mut self, index: usize, old: Note) {
        self.notes[index] = old;
    }

    fn active_count(&self) -> usize {
        self.notes.len() - self.tombstone.count_ones()
    }
}

// ─── 测试 1: 方案对比 - 逐位遍历 vs block-based ────────────

#[test]
fn bench_bitvec_vs_block() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let selected = generate_selection_50pct(count);

    eprintln!("\n═════ 逐位 vs block-based: {} 音符, 50% 选中 ═════", count);

    // 方案 A: 逐位遍历 (当前)
    {
        let mut store = NoteStore::new(notes.clone());
        let t = Instant::now();
        store.batch_move(&selected, 10.0, 3);
        eprintln!("  [逐位] 修改: {:?}", t.elapsed());
    }

    // 方案 B: block-based (只遍历选中位)
    {
        let mut store = NoteStore::new(notes.clone());
        let t = Instant::now();
        let num_blocks = selected.block_count();
        let block_chunk_size = num_blocks.div_ceil(8);
        let note_chunk_size = block_chunk_size * 64;
        let sel = &selected;

        // 模拟旧的逐位方式: 每次检查 selected.get(i)
        // 但用的是 BitSet 的 get() 方法
        std::thread::scope(|s| {
            for (thread_idx, chunk) in store.notes.chunks_mut(note_chunk_size).enumerate() {
                let block_start = thread_idx * block_chunk_size;
                let block_end = (block_start + block_chunk_size).min(num_blocks);
                s.spawn(move || {
                    for (local_bi, &block) in sel.blocks()[block_start..block_end].iter().enumerate() {
                        let base = local_bi * 64;
                        for local_i in 0..64 {
                            let global_i = base + local_i;
                            if global_i >= chunk.len() { break; }
                            if (block >> local_i) & 1 == 1 {
                                chunk[global_i].tick += 10.0;
                                chunk[global_i].key = (chunk[global_i].key as i32 + 3).clamp(0, 127) as u16;
                            }
                        }
                    }
                });
            }
        });
        let elapsed = t.elapsed();
        eprintln!("  [block逐位] 修改: {:?}", elapsed);
    }

    // 方案 C: trailing_zeros (只遍历选中位)
    {
        let mut store = NoteStore::new(notes);
        let t = Instant::now();
        let num_blocks = selected.block_count();
        let block_chunk_size = num_blocks.div_ceil(8);
        let note_chunk_size = block_chunk_size * 64;
        let sel = &selected;

        std::thread::scope(|s| {
            for (thread_idx, chunk) in store.notes.chunks_mut(note_chunk_size).enumerate() {
                let block_start = thread_idx * block_chunk_size;
                let block_end = (block_start + block_chunk_size).min(num_blocks);
                s.spawn(move || {
                    for (local_bi, &block) in sel.blocks()[block_start..block_end].iter().enumerate() {
                        let base = local_bi * 64;
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
        let elapsed = t.elapsed();
        eprintln!("  [trailing_zeros] 修改: {:?}", elapsed);
    }
}

// ─── 测试 2: 极致优化核心操作 ─────────────────────────────

#[test]
fn bench_optimized_ops() {
    let counts = [5_000_000, 10_000_000, 16_000_000];
    for &count in &counts {
        let notes = generate_notes(count);
        let selected = generate_selection_50pct(count);

        eprintln!("\n═════ 极致优化: {} 音符, 50% 选中 ═════", count);

        // 批量移动
        {
            let mut store = NoteStore::new(notes.clone());
            let t = Instant::now();
            store.batch_move(&selected, 10.0, 3);
            eprintln!("  [移动] 50%: {:?}", t.elapsed());
        }

        // 全选移动
        {
            let full = generate_selection_full(count);
            let mut store = NoteStore::new(notes.clone());
            let t = Instant::now();
            store.batch_move(&full, 10.0, 3);
            eprintln!("  [移动] 100%: {:?}", t.elapsed());
        }

        // 删除 (bulk OR)
        {
            let mut store = NoteStore::new(notes.clone());
            let t = Instant::now();
            let _undo = store.delete_selected(&selected);
            let del = t.elapsed();
            let undo_mem = count / 8 / (1024 * 1024);
            eprintln!("  [删除] 50%: {:?}, undo: ~{} MB", del, undo_mem);
        }

        // 撤销删除 (BitVec swap)
        {
            let mut store = NoteStore::new(notes);
            let saved = store.delete_selected(&selected);
            let t = Instant::now();
            store.undo_delete(saved);
            eprintln!("  [撤销删除] {:?}", t.elapsed());
        }
    }
}

// ─── 测试 3: 插入优化（预分配 vs 无预分配） ────────────────

#[test]
fn bench_insert_optimized() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let insert_notes: Vec<Note> = (0..1000).map(|i| Note::new(99999.0 + i as f32, 70, 5.0)).collect();

    eprintln!("\n═════ 插入优化: {} 基础音符 + {} 插入 ═════", count, insert_notes.len());

    // 无预分配 (模拟当前行为)
    {
        let mut store = NoteStore::new(notes.clone());
        // 手动移除 slack, 模拟原始行为
        store.notes.shrink_to_fit();
        let t = Instant::now();
        let start = store.notes.len();
        store.notes.extend_from_slice(&insert_notes);
        // 撤销
        store.notes.truncate(start);
        eprintln!("  [无预分配] 插入+撤销: {:?} (含 realloc)", t.elapsed());
    }

    // 有预分配 (10% slack)
    {
        let mut store = NoteStore::new(notes.clone());
        let t = Instant::now();
        let (start, _) = store.insert_notes(&insert_notes);
        store.undo_insert(start);
        eprintln!("  [有预分配] 插入+撤销: {:?}", t.elapsed());
    }

    // 直接 benchmark: 仅 extend (无 realloc)
    {
        let mut store = NoteStore::new(notes);
        store.notes.reserve(1000); // 确保有容量
        let t = Instant::now();
        store.notes.extend_from_slice(&insert_notes);
        eprintln!("  [纯extend] 仅插入: {:?}", t.elapsed());
    }
}

// ─── 测试 4: 完整工作流（极致优化版） ──────────────────────

#[test]
fn bench_full_workflow_optimized() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let selected = generate_selection_50pct(count);
    let mut store = NoteStore::new(notes);
    let insert_notes: Vec<Note> = (0..1000).map(|i| Note::new(99999.0 + i as f32, 70, 5.0)).collect();

    eprintln!("\n═════ 极致优化完整工作流: {} 音符 ═════", count);
    let mut total = std::time::Duration::ZERO;

    // 1. 批量移动 50%
    let t = Instant::now();
    store.batch_move(&selected, 10.0, 3);
    let e = t.elapsed(); total += e;
    eprintln!("  [1/5] 批量移动 50%: {:?}", e);

    // 2. 撤销移动
    let t = Instant::now();
    store.undo_move(&selected, 10.0, 3);
    let e = t.elapsed(); total += e;
    eprintln!("  [2/5] 撤销移动: {:?}", e);

    // 3. 删除 50% (bulk OR)
    let t = Instant::now();
    let saved = store.delete_selected(&selected);
    let e = t.elapsed(); total += e;
    eprintln!("  [3/5] 删除 50%: {:?}", e);

    // 4. 撤销删除 (BitVec swap)
    let t = Instant::now();
    store.undo_delete(saved);
    let e = t.elapsed(); total += e;
    eprintln!("  [4/5] 撤销删除: {:?}", e);

    // 5. 插入 1000 音符 (预分配)
    let t = Instant::now();
    let (start, _) = store.insert_notes(&insert_notes);
    store.undo_insert(start);
    let e = t.elapsed(); total += e;
    eprintln!("  [5/5] 插入+撤销 1000 音符: {:?}", e);
    drop(store);

    eprintln!("  ────────────────────────");
    eprintln!("  总耗时: {:?}", total);
    eprintln!("  峰值 undo 内存: ~{} MB (BitVec)",
        count / 8 / (1024 * 1024));
}

// ─── 测试 5: 100M 外推（极致优化后） ──────────────────────

#[test]
fn bench_100m_optimized() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let selected = generate_selection_50pct(count);

    eprintln!("\n═════ 100M 外推 (极致优化后) ═════");
    eprintln!("  16M 数据: {} MB", count * 16 / (1024 * 1024));
    eprintln!("  100M 数据: {} MB", (100_000_000 * 16) / (1024 * 1024));
    eprintln!("  100M undo 内存: {} MB (BitVec, 可逆操作)",
        (100_000_000 / 8) / (1024 * 1024));

    // 测试 16M 全选
    let full = generate_selection_full(count);
    {
        let mut store = NoteStore::new(notes.clone());
        let t = Instant::now();
        store.batch_move(&full, 10.0, 3);
        let _16m_full = t.elapsed();
        let factor = 100_000_000.0 / count as f64;
        eprintln!("\n  16M 全选修改: {:?}", _16m_full);
        eprintln!("  100M 全选修改预估: {:?}",
            std::time::Duration::from_secs_f64(_16m_full.as_secs_f64() * factor));
    }

    // 测试 16M 50%
    {
        let mut store = NoteStore::new(notes);
        let t = Instant::now();
        store.batch_move(&selected, 10.0, 3);
        let _16m_50 = t.elapsed();
        let factor = 100_000_000.0 / count as f64;
        eprintln!("\n  16M 50% 修改: {:?}", _16m_50);
        eprintln!("  100M 50% 修改预估: {:?}",
            std::time::Duration::from_secs_f64(_16m_50.as_secs_f64() * factor));
    }

    // 外推建议
    eprintln!("\n  ── 100M 建议 ──");
    eprintln!("  存储: Vec<Note> 直接存储, {} MB", (100_000_000 * 16) / (1024 * 1024));
    eprintln!("  修改: 8 线程 block-based, 只遍历选中位");
    eprintln!("  Undo: 可逆操作, 仅 {} MB bitvec", (100_000_000 / 8) / (1024 * 1024));
    eprintln!("  内存增量: 0 MB (原地修改)");
}

// ─── 测试 6: 音轨切换 + 序列化 ─────────────────────────────

#[test]
fn bench_track_switch_and_compact() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let selected = generate_selection_50pct(count);

    eprintln!("\n═════ 音轨切换 + 墓碑压缩: {} 音符 ═════", count);

    // 音轨切换: Arc<Vec<Note>>
    let arc = Arc::new(notes.clone());
    let t = Instant::now();
    let _cloned = Arc::clone(&arc);
    eprintln!("  音轨切换 (Arc::clone): {:?}", t.elapsed());
    drop(_cloned);

    // 墓碑压缩: 物理删除已标记的音符
    let mut store = NoteStore::new(notes);
    let _saved = store.delete_selected(&selected); // 删除 50%
    let t = Instant::now();
    let mut compacted = Vec::with_capacity(store.active_count());
    // 只保留未删除的音符
    for (i, note) in store.notes.iter().enumerate() {
        if !store.tombstone.get(i) {
            compacted.push(note.clone());
        }
    }
    let compact_time = t.elapsed();
    eprintln!("  墓碑压缩 (50% 删除): {:?}, 压缩后: {} 音符",
        compact_time, compacted.len());
    eprintln!("  压缩速率: {:.0}M/s",
        store.notes.len() as f64 / compact_time.as_secs_f64() / 1_000_000.0);
    drop(compacted);

    // 压缩后: 音轨切换用 Arc<Vec<Note>>
    let compacted_notes = store.notes.iter()
        .enumerate()
        .filter(|(i, _)| !store.tombstone.get(*i))
        .map(|(_, n)| n.clone())
        .collect::<Vec<_>>();
    let arc_compacted = Arc::new(compacted_notes);
    let t = Instant::now();
    let _arc_clone = Arc::clone(&arc_compacted);
    eprintln!("  压缩后音轨切换 (Arc::clone): {:?}", t.elapsed());
    drop(_arc_clone);
}

// ─── 测试 7: 单线程 vs 多线程 scaling ─────────────────────

#[test]
fn bench_thread_scaling() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let selected = generate_selection_50pct(count);
    let thread_counts = [1, 2, 4, 8, 16];

    eprintln!("\n═════ 线程扩展性: {} 音符, 50% 选中 ═════", count);
    eprintln!("  线程数  |  耗时  |  吞吐量");
    eprintln!("  ─────────────────────────");

    for &threads in &thread_counts {
        let mut store = NoteStore::new(notes.clone());
        let num_blocks = selected.block_count();
        let block_chunk_size = num_blocks.div_ceil(threads);
        let note_chunk_size = block_chunk_size * 64;
        let sel = &selected;

        let t = Instant::now();
        std::thread::scope(|s| {
            for (thread_idx, chunk) in store.notes.chunks_mut(note_chunk_size).enumerate() {
                let block_start = thread_idx * block_chunk_size;
                let block_end = (block_start + block_chunk_size).min(num_blocks);
                s.spawn(move || {
                    for (local_bi, &block) in sel.blocks()[block_start..block_end].iter().enumerate() {
                        let base = local_bi * 64;
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
        let elapsed = t.elapsed();
        let rate = (count as f64) / elapsed.as_secs_f64() / 1_000_000.0;
        eprintln!("  {:>3}     | {:>8?} | {:.0}M/s", threads, elapsed, rate);
    }
}

// ─── 测试 8: 稀疏选择场景（小部分选中） ────────────────────

#[test]
fn bench_sparse_selection() {
    let count = 16_000_000;
    let notes = generate_notes(count);

    eprintln!("\n═════ 稀疏选择场景: {} 音符 ═════", count);

    // 1% 选中
    let sparse_1pct = BitSet::from_fn(count, |i| i % 100 == 0);
    {
        let mut store = NoteStore::new(notes.clone());
        let t = Instant::now();
        store.batch_move(&sparse_1pct, 10.0, 3);
        eprintln!("  [1% 选中] {:?} (修改 {} 音符)", t.elapsed(), sparse_1pct.count_ones());
    }

    // 0.1% 选中
    let sparse_01pct = BitSet::from_fn(count, |i| i % 1000 == 0);
    {
        let mut store = NoteStore::new(notes.clone());
        let t = Instant::now();
        store.batch_move(&sparse_01pct, 10.0, 3);
        eprintln!("  [0.1% 选中] {:?} (修改 {} 音符)", t.elapsed(), sparse_01pct.count_ones());
    }

    // 0.01% 选中
    let sparse_001pct = BitSet::from_fn(count, |i| i % 10000 == 0);
    {
        let mut store = NoteStore::new(notes);
        let t = Instant::now();
        store.batch_move(&sparse_001pct, 10.0, 3);
        eprintln!("  [0.01% 选中] {:?} (修改 {} 音符)", t.elapsed(), sparse_001pct.count_ones());
    }
}