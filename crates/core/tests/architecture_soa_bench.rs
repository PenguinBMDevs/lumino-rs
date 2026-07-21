//! 新架构: SoA (Struct of Arrays) 极致优化
//!
//! 核心思想: 将 Note 拆分为独立的 Vec<f32>, Vec<u16>, Vec<u8> 数组
//! 每次操作只触摸需要修改的字段, 减少 2.7x 内存流量
//!
//! 运行: cargo test --release -p lumino-core --test architecture_soa_bench -- --nocapture

use std::sync::Arc;
use std::time::Instant;

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
            if f(i) { set.set(i); }
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
    fn clone_blocks(&self) -> Vec<u64> { self.blocks.clone() }
    fn count_ones(&self) -> usize { self.blocks.iter().map(|b| b.count_ones() as usize).sum() }
    fn len(&self) -> usize { self.len }
}

fn generate_selection_50pct(count: usize) -> BitSet {
    BitSet::from_fn(count, |i| i % 2 == 0)
}

fn generate_selection_full(count: usize) -> BitSet {
    let mut set = BitSet::new(count);
    for b in set.blocks.iter_mut() { *b = !0; }
    let rem = count % 64;
    if rem > 0 { set.blocks[set.blocks.len() - 1] &= (1 << rem) - 1; }
    set
}

// ─── SoA (Struct of Arrays) 核心存储 ────────────────────────

struct SoANoteStore {
    // 5 个独立 Vec, 每次操作只触摸需要的
    ticks: Vec<f32>,        // 16M × 4 = 64MB
    keys: Vec<u16>,         // 16M × 2 = 32MB
    lengths: Vec<f32>,      // 16M × 4 = 64MB
    velocities: Vec<u8>,    // 16M × 1 = 16MB
    channels: Vec<u8>,      // 16M × 1 = 16MB
    // 总数据: 192MB (vs AoS 256MB)

    tombstone: BitSet,       // 2MB
    selection: BitSet,        // 2MB

    // 预分配 slack, 避免 insert 触发 realloc
    slack: usize,
}

impl SoANoteStore {
    fn new(note_count: usize) -> Self {
        let slack = (note_count / 10).max(1024);
        let mut store = Self {
            ticks: Vec::with_capacity(note_count + slack),
            keys: Vec::with_capacity(note_count + slack),
            lengths: Vec::with_capacity(note_count + slack),
            velocities: Vec::with_capacity(note_count + slack),
            channels: Vec::with_capacity(note_count + slack),
            tombstone: BitSet::new(note_count),
            selection: BitSet::new(note_count),
            slack,
        };
        // 初始化数据
        for i in 0..note_count {
            store.ticks.push(i as f32 * 10.0);
            store.keys.push(60 + (i % 24) as u16);
            store.lengths.push(5.0);
            store.velocities.push(100);
            store.channels.push(0);
        }
        store
    }

    // ─── 批量移动: 只触摸 ticks + keys (2/5 字段) ──

    fn batch_move(&mut self, selected: &BitSet, delta_tick: f32, delta_key: i16) {
        let n = self.ticks.len();
        let num_blocks = selected.block_count();
        let block_chunk_size = num_blocks.div_ceil(8);
        let chunk_size = block_chunk_size * 64;

        // 并行修改 ticks (连续内存, 极优缓存)
        std::thread::scope(|s| {
            for (thread_idx, chunk) in self.ticks.chunks_mut(chunk_size).enumerate() {
                let block_start = thread_idx * block_chunk_size;
                let block_end = (block_start + block_chunk_size).min(num_blocks);
                let sel = &selected;
                s.spawn(move || {
                    for (local_bi, &block) in sel.blocks()[block_start..block_end].iter().enumerate() {
                        let base = local_bi * 64;
                        let mut bits = block;
                        while bits != 0 {
                            let tz = bits.trailing_zeros();
                            let idx = base + tz as usize;
                            if idx < chunk.len() {
                                chunk[idx] = (chunk[idx] + delta_tick).max(0.0);
                            }
                            bits &= bits - 1;
                        }
                    }
                });
            }
        });

        // 并行修改 keys (连续内存)
        std::thread::scope(|s| {
            for (thread_idx, chunk) in self.keys.chunks_mut(chunk_size).enumerate() {
                let block_start = thread_idx * block_chunk_size;
                let block_end = (block_start + block_chunk_size).min(num_blocks);
                let sel = &selected;
                s.spawn(move || {
                    for (local_bi, &block) in sel.blocks()[block_start..block_end].iter().enumerate() {
                        let base = local_bi * 64;
                        let mut bits = block;
                        while bits != 0 {
                            let tz = bits.trailing_zeros();
                            let idx = base + tz as usize;
                            if idx < chunk.len() {
                                chunk[idx] = (chunk[idx] as i32 + delta_key as i32).clamp(0, 127) as u16;
                            }
                            bits &= bits - 1;
                        }
                    }
                });
            }
        });
    }

    // ─── 批量移动: 顺序遍历版 (更优缓存, 适合稠密选择) ──

    fn batch_move_sequential(&mut self, selected: &BitSet, delta_tick: f32, delta_key: i16) {
        let n = self.ticks.len();
        let chunk_size = n.div_ceil(8);

        // 顺序遍历 ticks (L1/L2 缓存友好)
        std::thread::scope(|s| {
            for (offset, chunk) in self.ticks.chunks_mut(chunk_size).enumerate() {
                let base = offset * chunk_size;
                let sel = &selected;
                s.spawn(move || {
                    for (i, tick) in chunk.iter_mut().enumerate() {
                        let gi = base + i;
                        if gi < sel.len() && sel.get(gi) {
                            *tick = (*tick + delta_tick).max(0.0);
                        }
                    }
                });
            }
        });

        // 顺序遍历 keys
        std::thread::scope(|s| {
            for (offset, chunk) in self.keys.chunks_mut(chunk_size).enumerate() {
                let base = offset * chunk_size;
                let sel = &selected;
                s.spawn(move || {
                    for (i, key) in chunk.iter_mut().enumerate() {
                        let gi = base + i;
                        if gi < sel.len() && sel.get(gi) {
                            *key = (*key as i32 + delta_key as i32).clamp(0, 127) as u16;
                        }
                    }
                });
            }
        });
    }

    // 撤销移动
    fn undo_move(&mut self, selected: &BitSet, delta_tick: f32, delta_key: i16) {
        self.batch_move(selected, -delta_tick, -delta_key);
    }

    // ─── 墓碑删除 (bulk OR) ──

    fn delete_selected(&mut self, selected: &BitSet) -> Vec<u64> {
        let undo = self.tombstone.clone_blocks();
        for (t, s) in self.tombstone.blocks_mut().iter_mut().zip(selected.blocks().iter()) {
            *t |= s;
        }
        undo
    }

    fn undo_delete(&mut self, saved_blocks: Vec<u64>) {
        self.tombstone.blocks = saved_blocks;
    }

    // ─── 插入: 5 个 Vec 各自 extend, 每个只增长 12KB ──

    fn insert_notes(&mut self, count: usize) -> usize {
        let start = self.ticks.len();
        let needed = start + count;
        let slack = self.slack;

        // 每个 Vec 独立增长, 避免一次性 realloc 256MB
        macro_rules! ensure_cap {
            ($vec:expr) => {
                if $vec.capacity() < needed {
                    let new_cap = (needed * 11) / 10 + slack;
                    $vec.reserve(new_cap - $vec.len());
                }
            };
        }

        ensure_cap!(self.ticks);
        ensure_cap!(self.keys);
        ensure_cap!(self.lengths);
        ensure_cap!(self.velocities);
        ensure_cap!(self.channels);

        // 填充默认值
        for i in 0..count {
            self.ticks.push(0.0);
            self.keys.push(60);
            self.lengths.push(5.0);
            self.velocities.push(100);
            self.channels.push(0);
        }

        // 扩展 tombstone
        self.tombstone.blocks.resize(self.ticks.len().div_ceil(64), 0);
        start
    }

    fn undo_insert(&mut self, start_idx: usize) {
        self.ticks.truncate(start_idx);
        self.keys.truncate(start_idx);
        self.lengths.truncate(start_idx);
        self.velocities.truncate(start_idx);
        self.channels.truncate(start_idx);
        let new_blocks = start_idx.div_ceil(64);
        self.tombstone.blocks.truncate(new_blocks);
    }

    fn len(&self) -> usize { self.ticks.len() }
    fn memory_mb(&self) -> f64 {
        let data = self.ticks.capacity() * 4
            + self.keys.capacity() * 2
            + self.lengths.capacity() * 4
            + self.velocities.capacity() * 1
            + self.channels.capacity() * 1;
        let bitset = self.tombstone.blocks.capacity() * 8
            + self.selection.blocks.capacity() * 8;
        (data + bitset) as f64 / (1024.0 * 1024.0)
    }
}

// ─── AoS (Array of Structs) 当前架构对比 ───────────────────

#[derive(Clone, Debug, PartialEq)]
#[repr(C)]
struct Note {
    tick: f32;
    key: u16;
    length: f32;
    velocity: u8;
    channel: u8;
}

struct AoSNoteStore {
    notes: Vec<Note>,
    tombstone: BitSet,
}

impl AoSNoteStore {
    fn new(note_count: usize) -> Self {
        let slack = (note_count / 10).max(1024);
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
        Self { notes, tombstone: BitSet::new(note_count) }
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
                    for (local_bi, &block) in sel.blocks()[block_start..block_end].iter().enumerate() {
                        let base = local_bi * 64;
                        let mut bits = block;
                        while bits != 0 {
                            let tz = bits.trailing_zeros();
                            let idx = base + tz as usize;
                            if idx < chunk.len() {
                                chunk[idx].tick = (chunk[idx].tick + delta_tick).max(0.0);
                                chunk[idx].key = (chunk[idx].key as i32 + delta_key as i32).clamp(0, 127) as u16;
                            }
                            bits &= bits - 1;
                        }
                    }
                });
            }
        });
    }

    fn memory_mb(&self) -> f64 {
        let data = self.notes.capacity() * std::mem::size_of::<Note>();
        let bitset = self.tombstone.blocks.capacity() * 8;
        (data + bitset) as f64 / (1024.0 * 1024.0)
    }

    fn len(&self) -> usize { self.notes.len() }
}

// ─── 测试 1: 方案对比 - AoS vs SoA (trailing_zeros) vs SoA (sequential) ──

#[test]
fn bench_soa_vs_aos() {
    let counts = [5_000_000, 10_000_000, 16_000_000];
    for &count in &counts {
        let selected = generate_selection_50pct(count);

        eprintln!("\n═════ SoA vs AoS: {} 音符, 50% 选中 ═════", count);

        // AoS (trailing_zeros)
        {
            let mut store = AoSNoteStore::new(count);
            let t = Instant::now();
            store.batch_move(&selected, 10.0, 3);
            eprintln!("  [AoS trailing_zeros] {:?}, 内存: {:.0} MB",
                t.elapsed(), store.memory_mb());
        }

        // SoA (trailing_zeros)
        {
            let mut store = SoANoteStore::new(count);
            let t = Instant::now();
            store.batch_move(&selected, 10.0, 3);
            eprintln!("  [SoA trailing_zeros] {:?}, 内存: {:.0} MB",
                t.elapsed(), store.memory_mb());
        }

        // SoA (sequential)
        {
            let mut store = SoANoteStore::new(count);
            let t = Instant::now();
            store.batch_move_sequential(&selected, 10.0, 3);
            eprintln!("  [SoA sequential]     {:?}, 内存: {:.0} MB",
                t.elapsed(), store.memory_mb());
        }
    }
}

// ─── 测试 2: 全选场景 ─────────────────────────────────────

#[test]
fn bench_soa_full_selection() {
    let counts = [5_000_000, 10_000_000, 16_000_000];
    for &count in &counts {
        let selected = generate_selection_full(count);

        eprintln!("\n═════ SoA 全选: {} 音符 ═════", count);

        // SoA sequential (全选时 sequential 最优, 无分支)
        {
            let mut store = SoANoteStore::new(count);
            let t = Instant::now();
            store.batch_move_sequential(&selected, 10.0, 3);
            eprintln!("  [sequential] {:?}", t.elapsed());
        }

        // SoA trailing_zeros (全选时 trailing_zeros 遍历所有位)
        {
            let mut store = SoANoteStore::new(count);
            let t = Instant::now();
            store.batch_move(&selected, 10.0, 3);
            eprintln!("  [trailing_zeros] {:?}", t.elapsed());
        }
    }
}

// ─── 测试 3: 插入优化 (SoA 5个小Vec各自增长) ─────────────

#[test]
fn bench_soa_insert() {
    let count = 16_000_000;
    let insert_count = 1000;

    eprintln!("\n═════ SoA 插入: {} 基础 + {} 插入 ═════", count, insert_count);

    // SoA 插入 (预分配)
    {
        let mut store = SoANoteStore::new(count);
        let t = Instant::now();
        let start = store.insert_notes(insert_count);
        store.undo_insert(start);
        eprintln!("  [SoA 预分配] 插入+撤销: {:?}", t.elapsed());
        eprintln!("    SoA 内存: {:.0} MB", store.memory_mb());
    }

    // AoS 插入 (预分配)
    {
        let mut store = AoSNoteStore::new(count);
        let t = Instant::now();
        let start = store.len();
        store.notes.reserve(insert_count);
        for _ in 0..insert_count {
            store.notes.push(Note { tick: 0.0, key: 60, length: 5.0, velocity: 100, channel: 0 });
        }
        store.notes.truncate(start);
        eprintln!("  [AoS 预分配] 插入+撤销: {:?}", t.elapsed());
        eprintln!("    AoS 内存: {:.0} MB", store.memory_mb());
    }

    // 大批量插入: 100K 音符
    let insert_100k = 100_000;
    eprintln!("\n  ── 大批量插入: {} 音符 ──", insert_100k);
    {
        let mut store = SoANoteStore::new(count);
        let t = Instant::now();
        let start = store.insert_notes(insert_100k);
        store.undo_insert(start);
        eprintln!("  [SoA 100K] 插入+撤销: {:?}", t.elapsed());
    }
    {
        let mut store = AoSNoteStore::new(count);
        let t = Instant::now();
        let start = store.len();
        store.notes.reserve(insert_100k);
        for _ in 0..insert_100k {
            store.notes.push(Note { tick: 0.0, key: 60, length: 5.0, velocity: 100, channel: 0 });
        }
        store.notes.truncate(start);
        eprintln!("  [AoS 100K] 插入+撤销: {:?}", t.elapsed());
    }
}

// ─── 测试 4: 完整工作流 (SoA sequential) ──────────────────

#[test]
fn bench_soa_full_workflow() {
    let count = 16_000_000;
    let selected = generate_selection_50pct(count);
    let mut store = SoANoteStore::new(count);

    eprintln!("\n═════ SoA 完整工作流: {} 音符 ═════", count);
    let mut total = std::time::Duration::ZERO;

    // 1. 批量移动 50% (sequential)
    let t = Instant::now();
    store.batch_move_sequential(&selected, 10.0, 3);
    let e = t.elapsed(); total += e;
    eprintln!("  [1/5] 批量移动 50%: {:?}", e);

    // 2. 撤销移动
    let t = Instant::now();
    store.undo_move(&selected, 10.0, 3);
    let e = t.elapsed(); total += e;
    eprintln!("  [2/5] 撤销移动: {:?}", e);

    // 3. 删除 50%
    let t = Instant::now();
    let saved = store.delete_selected(&selected);
    let e = t.elapsed(); total += e;
    eprintln!("  [3/5] 删除 50%: {:?}", e);

    // 4. 撤销删除
    let t = Instant::now();
    store.undo_delete(saved);
    let e = t.elapsed(); total += e;
    eprintln!("  [4/5] 撤销删除: {:?}", e);

    // 5. 插入 1000 音符
    let t = Instant::now();
    let start = store.insert_notes(1000);
    store.undo_insert(start);
    let e = t.elapsed(); total += e;
    eprintln!("  [5/5] 插入 1000 音符: {:?}", e);

    eprintln!("  ────────────────────────");
    eprintln!("  总耗时: {:?}", total);
    eprintln!("  内存: {:.0} MB", store.memory_mb());
}

// ─── 测试 5: 100M 外推 ─────────────────────────────────────

#[test]
fn bench_soa_100m_extrapolation() {
    let count = 16_000_000;
    let selected_50 = generate_selection_50pct(count);
    let selected_full = generate_selection_full(count);

    eprintln!("\n═════ SoA 100M 外推 ═════");
    eprintln!("  16M SoA 数据: {:.0} MB", (16_000_000 * 12) as f64 / (1024.0 * 1024.0));
    eprintln!("  100M SoA 数据: {:.0} MB", (100_000_000 * 12) as f64 / (1024.0 * 1024.0));
    eprintln!("  100M AoS 数据: {:.0} MB", (100_000_000 * 16) as f64 / (1024.0 * 1024.0));
    eprintln!("  100M undo 内存: {} MB (BitVec, 可逆操作)",
        (100_000_000 / 8) / (1024 * 1024));
    let factor = 100_000_000.0 / count as f64;

    // 50% 选中 (sequential)
    {
        let mut store = SoANoteStore::new(count);
        let t = Instant::now();
        store.batch_move_sequential(&selected_50, 10.0, 3);
        let _16m = t.elapsed();
        eprintln!("\n  16M 50% (sequential): {:?}", _16m);
        eprintln!("  100M 50% (sequential): {:?}",
            std::time::Duration::from_secs_f64(_16m.as_secs_f64() * factor));
    }

    // 全选 (sequential)
    {
        let mut store = SoANoteStore::new(count);
        let t = Instant::now();
        store.batch_move_sequential(&selected_full, 10.0, 3);
        let _16m = t.elapsed();
        eprintln!("\n  16M 全选 (sequential): {:?}", _16m);
        eprintln!("  100M 全选 (sequential): {:?}",
            std::time::Duration::from_secs_f64(_16m.as_secs_f64() * factor));
    }

    // 50% 选中 (trailing_zeros)
    {
        let mut store = SoANoteStore::new(count);
        let t = Instant::now();
        store.batch_move(&selected_50, 10.0, 3);
        let _16m = t.elapsed();
        eprintln!("\n  16M 50% (trailing_zeros): {:?}", _16m);
        eprintln!("  100M 50% (trailing_zeros): {:?}",
            std::time::Duration::from_secs_f64(_16m.as_secs_f64() * factor));
    }

    // 理论极限
    eprintln!("\n  ── 理论极限 (40GB/s DDR4) ──");
    let soa_ticks_mb = (100_000_000 * 4) as f64 / (1024.0 * 1024.0);
    let soa_keys_mb = (100_000_000 * 2) as f64 / (1024.0 * 1024.0);
    let soa_read = soa_ticks_mb + soa_keys_mb; // 读取所有 tick + key
    let soa_write = soa_ticks_mb / 2.0 + soa_keys_mb / 2.0; // 修改 50%
    eprintln!("  SoA 内存流量: {:.0}MB read + {:.0}MB write = {:.0}MB total",
        soa_read, soa_write, soa_read + soa_write);
    eprintln!("  理论极限: {:.1}ms", (soa_read + soa_write) * 1024.0 * 1024.0 / (40.0 * 1024.0 * 1024.0 * 1024.0) * 1000.0);
}

// ─── 测试 6: 线程扩展性 (SoA sequential) ───────────────────

#[test]
fn bench_soa_thread_scaling() {
    let count = 16_000_000;
    let selected = generate_selection_50pct(count);
    let thread_counts = [1, 2, 4, 8, 16];

    eprintln!("\n═════ SoA 线程扩展性: {} 音符, 50% 选中 ═════", count);
    eprintln!("  线程数  |  耗时        |  吞吐量");

    for &threads in &thread_counts {
        let mut store = SoANoteStore::new(count);
        let n = store.len();
        let chunk_size = n.div_ceil(threads);

        let t = Instant::now();
        std::thread::scope(|s| {
            for (offset, chunk) in store.ticks.chunks_mut(chunk_size).enumerate() {
                let base = offset * chunk_size;
                let sel = &selected;
                s.spawn(move || {
                    for (i, tick) in chunk.iter_mut().enumerate() {
                        let gi = base + i;
                        if gi < sel.len() && sel.get(gi) {
                            *tick = (*tick + 10.0).max(0.0);
                        }
                    }
                });
            }
        });

        // 只测 ticks (keys 同理, 省略)
        std::thread::scope(|s| {
            for (offset, chunk) in store.keys.chunks_mut(chunk_size).enumerate() {
                let base = offset * chunk_size;
                let sel = &selected;
                s.spawn(move || {
                    for (i, key) in chunk.iter_mut().enumerate() {
                        let gi = base + i;
                        if gi < sel.len() && sel.get(gi) {
                            *key = (*key as i32 + 3).clamp(0, 127) as u16;
                        }
                    }
                });
            }
        });

        let elapsed = t.elapsed();
        eprintln!("  {:>3}     | {:>10?} | {:.0}M/s", threads, elapsed, (count as f64) / elapsed.as_secs_f64() / 1_000_000.0);
    }
}

// ─── 测试 7: 内存对比 ─────────────────────────────────────

#[test]
fn bench_memory_comparison() {
    let count = 16_000_000;

    eprintln!("\n═════ 内存对比: {} 音符 ═════", count);
    eprintln!("");
    eprintln!("  ┌─ 类别 ───────────┬─ SoA ────┬─ AoS ────┬─ 节省 ─┐");

    let soa_data = count * 12;
    let aos_data = count * 16;
    eprintln!("  │ 数据              │ {:>4} MB │ {:>4} MB │ {:>4} MB │",
        soa_data / (1024 * 1024),
        aos_data / (1024 * 1024),
        (aos_data - soa_data) / (1024 * 1024));

    let soa_slack = (count / 10) * 12;
    let aos_slack = (count / 10) * 16;
    eprintln!("  │ 10% slack         │ {:>4} MB │ {:>4} MB │ {:>4} MB │",
        soa_slack / (1024 * 1024),
        aos_slack / (1024 * 1024),
        (aos_slack - soa_slack) / (1024 * 1024));

    let bitset = count / 8;
    eprintln!("  │ BitSet ×2         │ {:>4} MB │ {:>4} MB │ 0 MB    │",
        bitset * 2 / (1024 * 1024),
        bitset * 2 / (1024 * 1024));

    let soa_total = (soa_data + soa_slack + bitset * 2) as f64;
    let aos_total = (aos_data + aos_slack + bitset * 2) as f64;
    eprintln!("  ├─ 合计 ───────────┼─ {:>4} ──┼─ {:>4} ──┼─ {:>4} ─┤",
        (soa_total / (1024.0 * 1024.0)) as u32,
        (aos_total / (1024.0 * 1024.0)) as u32,
        ((aos_total - soa_total) / (1024.0 * 1024.0)) as u32);

    // 音轨切换
    let soa_track = soa_data; // Arc 共享, 只算数据
    let aos_track = aos_data;
    eprintln!("  │ 音轨切换(峰值)    │ {:>4} MB │ {:>4} MB │ {:>4} MB │",
        (soa_data + soa_track) / (1024 * 1024),
        (aos_data + aos_track) / (1024 * 1024),
        (aos_data + aos_track - soa_data - soa_track) / (1024 * 1024));

    // 100M
    let soa_100m = (100_000_000 * 12) / (1024 * 1024);
    let aos_100m = (100_000_000 * 16) / (1024 * 1024);
    eprintln!("  │ 100M 数据         │ {:>4} MB │ {:>4} MB │ {:>4} MB │",
        soa_100m, aos_100m, aos_100m - soa_100m);

    eprintln!("  └──────────────────┴─────────┴─────────┴─────────┘");
}