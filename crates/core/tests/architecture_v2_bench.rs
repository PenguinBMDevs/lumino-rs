//! 新架构 v2: 完整场景 release mode 基准测试
//!
//! 包含：batch drag/delete/insert/single modify + undo/redo
//! 运行：cargo test --release -p lumino-core --test architecture_v2_bench -- --nocapture

use bit_vec::BitVec;
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
        Self {
            tick,
            key,
            length,
            velocity: 100,
            channel: 0,
        }
    }
}

fn generate_notes(count: usize) -> Vec<Note> {
    (0..count)
        .map(|i| Note::new(i as f32 * 10.0, 60 + (i % 24) as u16, 5.0))
        .collect()
}

fn generate_selection_50pct(count: usize) -> BitVec {
    let mut bv = BitVec::from_elem(count, false);
    for i in (0..count).step_by(2) {
        bv.set(i, true);
    }
    bv
}

fn generate_selection_full(count: usize) -> BitVec {
    BitVec::from_elem(count, true)
}

// ─── 可逆操作：Undo 不需要保存数据，只保存操作本身 ───────────

/// 可逆操作：undo 时只需反向应用
#[derive(Clone)]
struct ReversibleMove {
    selected: BitVec, // 哪些音符被选中移动
    delta_tick: f32,
    delta_key: i16,
}

/// 不可逆操作：需要保存原始数据
#[derive(Clone)]
struct IrreversibleOp {
    indices: Vec<usize>,
    old_notes: Vec<Note>, // 删除/修改前的原始数据
}

/// 墓碑删除：不实际删除数据，只标记
struct TombstoneDelete {
    was_deleted: BitVec, // 删除前的墓碑状态
}

// ─── 方案 A: Vec<Note> + 原地修改 + 可逆 undo ──────────────

struct NoteStoreV1 {
    notes: Vec<Note>,
    tombstone: BitVec, // 1 = 已删除（不可见），0 = 活跃
}

impl NoteStoreV1 {
    fn new(notes: Vec<Note>) -> Self {
        let count = notes.len();
        Self {
            notes,
            tombstone: BitVec::from_elem(count, false),
        }
    }

    /// 批量移动（可逆操作）
    fn batch_move(&mut self, selected: &BitVec, delta_tick: f32, delta_key: i16) -> ReversibleMove {
        let undo = ReversibleMove {
            selected: selected.clone(),
            delta_tick,
            delta_key,
        };
        // 原地并行修改
        let chunk_size = self.notes.len().div_ceil(8);
        std::thread::scope(|s| {
            let mut handles = Vec::with_capacity(8);
            for (chunk_idx, chunk) in self.notes.chunks_mut(chunk_size).enumerate() {
                let start = chunk_idx * chunk_size;
                handles.push(s.spawn(move || {
                    for (local_i, note) in chunk.iter_mut().enumerate() {
                        let gi = start + local_i;
                        if gi >= selected.len() || !selected[gi] {
                            continue;
                        }
                        note.tick = (note.tick + delta_tick).max(0.0);
                        note.key = (note.key as i32 + delta_key as i32).clamp(0, 127) as u16;
                    }
                }));
            }
            for h in handles {
                h.join().expect("工作线程 join 应成功");
            }
        });
        undo
    }

    /// 撤销批量移动
    fn undo_move(&mut self, op: &ReversibleMove) {
        self.batch_move(&op.selected, -op.delta_tick, -op.delta_key);
    }

    /// 删除选中音符（墓碑方式）
    fn delete_selected(&mut self, selected: &BitVec) -> TombstoneDelete {
        let undo = TombstoneDelete {
            was_deleted: self.tombstone.clone(),
        };
        for i in 0..selected.len().min(self.tombstone.len()) {
            if selected[i] {
                self.tombstone.set(i, true);
            }
        }
        undo
    }

    /// 撤销删除
    fn undo_delete(&mut self, op: &TombstoneDelete) {
        self.tombstone = op.was_deleted.clone();
    }

    /// 插入音符（在末尾追加）
    fn insert_notes(&mut self, new_notes: &[Note]) -> IrreversibleOp {
        let start_idx = self.notes.len();
        self.notes.extend_from_slice(new_notes);
        self.tombstone.grow(new_notes.len(), false);
        IrreversibleOp {
            indices: (start_idx..self.notes.len()).collect(),
            old_notes: new_notes.to_vec(),
        }
    }

    /// 撤销插入
    fn undo_insert(&mut self, op: &IrreversibleOp) {
        // 截断到插入前的位置
        let new_len = op.indices[0];
        self.notes.truncate(new_len);
        self.tombstone.truncate(new_len);
    }

    /// 修改单个音符
    fn modify_note(
        &mut self,
        index: usize,
        new_tick: f32,
        new_key: u16,
        new_len: f32,
    ) -> Box<Note> {
        let old = Box::new(self.notes[index].clone());
        let note = &mut self.notes[index];
        note.tick = new_tick;
        note.key = new_key;
        note.length = new_len;
        old
    }

    /// 撤销单个修改
    fn undo_modify(&mut self, index: usize, old: &Note) {
        self.notes[index] = old.clone();
    }

    /// 获取活跃（未删除）音符数
    fn active_count(&self) -> usize {
        self.notes.len() - self.tombstone.iter().filter(|&d| d).count()
    }
}

// ─── 方案 B: Arc<Vec<Note>> + 快照 undo ────────────────────

struct NoteStoreV2 {
    notes: Arc<Vec<Note>>,
    undo_stack: Vec<Arc<Vec<Note>>>,
    redo_stack: Vec<Arc<Vec<Note>>>,
    max_undo: usize,
}

impl NoteStoreV2 {
    fn new(notes: Vec<Note>, max_undo: usize) -> Self {
        Self {
            notes: Arc::new(notes),
            undo_stack: Vec::with_capacity(max_undo),
            redo_stack: Vec::new(),
            max_undo,
        }
    }

    fn checkpoint(&mut self) {
        self.undo_stack.push(Arc::clone(&self.notes));
        if self.undo_stack.len() > self.max_undo {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn commit(&mut self, new_notes: Vec<Note>) {
        self.notes = Arc::new(new_notes);
    }

    fn undo(&mut self) -> bool {
        if let Some(old) = self.undo_stack.pop() {
            self.redo_stack.push(Arc::clone(&self.notes));
            self.notes = old;
            true
        } else {
            false
        }
    }

    fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(Arc::clone(&self.notes));
            self.notes = next;
            true
        } else {
            false
        }
    }
}

// ─── 测试 1: 可逆操作 undo 开销（核心卖点） ────────────────

#[test]
fn bench_reversible_undo_cost() {
    let counts = [5_000_000, 10_000_000, 16_000_000];
    for &count in &counts {
        eprintln!("\n═════ 可逆操作 undo 开销: {} 音符, 50% 选中 ═════", count);
        let notes = generate_notes(count);
        let selected = generate_selection_50pct(count);
        let mut store = NoteStoreV1::new(notes);

        // 1. 执行批量移动
        let t0 = Instant::now();
        let op = store.batch_move(&selected, 10.0, 3);
        let exec_time = t0.elapsed();
        eprintln!(
            "  执行: {:?}, undo 内存: ~{} MB (BitVec)",
            exec_time,
            count / 8 / (1024 * 1024)
        );

        // 2. 撤销
        let t1 = Instant::now();
        store.undo_move(&op);
        let undo_time = t1.elapsed();
        eprintln!("  撤销: {:?} (与执行相同)", undo_time);

        // 3. 重做
        let t2 = Instant::now();
        store.batch_move(&op.selected, op.delta_tick, op.delta_key);
        let redo_time = t2.elapsed();
        eprintln!("  重做: {:?} (与执行相同)", redo_time);

        eprintln!("  undo 总开销: 0 MB (只存了操作本身, 2MB BitVec)");
    }
}

// ─── 测试 2: 墓碑删除性能 ─────────────────────────────────

#[test]
fn bench_tombstone_delete() {
    let counts = [5_000_000, 10_000_000, 16_000_000];
    for &count in &counts {
        eprintln!("\n═════ 墓碑删除: {} 音符, 50% 选中 ═════", count);
        let notes = generate_notes(count);
        let selected = generate_selection_50pct(count);
        let mut store = NoteStoreV1::new(notes);

        // 1. 删除（墓碑方式）
        let t0 = Instant::now();
        let op = store.delete_selected(&selected);
        let del_time = t0.elapsed();
        let undo_mem = count / 8 / (1024 * 1024);
        eprintln!(
            "  删除: {:?}, undo 内存: ~{} MB (BitVec)",
            del_time, undo_mem
        );
        eprintln!(
            "  活跃音符: {} / {}",
            store.active_count(),
            store.notes.len()
        );

        // 2. 撤销删除
        let t1 = Instant::now();
        store.undo_delete(&op);
        let undo_time = t1.elapsed();
        eprintln!("  撤销删除: {:?}", undo_time);
        eprintln!("  恢复后活跃音符: {}", store.active_count());
    }
}

// ─── 测试 3: 插入 + 单音符修改 ─────────────────────────────

#[test]
fn bench_insert_and_modify() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let mut store = NoteStoreV1::new(notes);

    // 插入 1000 个音符
    eprintln!(
        "\n═════ 插入 + 单音符修改: {} 基础音符 + 1000 插入 ═════",
        count
    );
    let insert_notes: Vec<Note> = (0..1000)
        .map(|i| Note::new(100000.0 + i as f32, 70, 5.0))
        .collect();

    let t0 = Instant::now();
    let op = store.insert_notes(&insert_notes);
    let ins_time = t0.elapsed();
    let ins_mem = 1000 * 16;
    eprintln!(
        "  插入 1000 音符: {:?}, undo 内存: {} bytes",
        ins_time, ins_mem
    );

    // 撤销插入
    let t1 = Instant::now();
    store.undo_insert(&op);
    let undo_time = t1.elapsed();
    eprintln!("  撤销插入: {:?}", undo_time);
    eprintln!("  撤销后音符数: {}", store.notes.len());

    // 修改单个音符
    let t2 = Instant::now();
    let old = store.modify_note(0, 999.0, 100, 10.0);
    let mod_time = t2.elapsed();
    eprintln!("  修改单音符: {:?}, undo 内存: {} bytes", mod_time, 16);

    // 撤销修改
    let t3 = Instant::now();
    store.undo_modify(0, &old);
    let undo_mod_time = t3.elapsed();
    eprintln!("  撤销修改: {:?}", undo_mod_time);
}

// ─── 测试 4: 完整工作流（模拟真实用户操作序列） ────────────

#[test]
fn bench_full_workflow() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let mut store = NoteStoreV1::new(notes);
    let selected_50 = generate_selection_50pct(count);

    eprintln!("\n═════ 完整工作流: {} 音符 ═════", count);
    let mut total_time = std::time::Duration::ZERO;

    // 1. 批量移动 (50% 选中)
    let t = Instant::now();
    let op1 = store.batch_move(&selected_50, 10.0, 3);
    let elapsed = t.elapsed();
    total_time += elapsed;
    eprintln!("  [1/5] 批量移动 50%: {:?}", elapsed);

    // 2. 撤销移动
    let t = Instant::now();
    store.undo_move(&op1);
    let elapsed = t.elapsed();
    total_time += elapsed;
    eprintln!("  [2/5] 撤销移动: {:?}", elapsed);

    // 3. 删除 50%
    let t = Instant::now();
    let op3 = store.delete_selected(&selected_50);
    let elapsed = t.elapsed();
    total_time += elapsed;
    eprintln!("  [3/5] 删除 50%: {:?}", elapsed);

    // 4. 撤销删除
    let t = Instant::now();
    store.undo_delete(&op3);
    let elapsed = t.elapsed();
    total_time += elapsed;
    eprintln!("  [4/5] 撤销删除: {:?}", elapsed);

    // 5. 插入 1000 音符
    let insert_notes: Vec<Note> = (0..1000)
        .map(|i| Note::new(99999.0 + i as f32, 70, 5.0))
        .collect();
    let t = Instant::now();
    let op5 = store.insert_notes(&insert_notes);
    let elapsed = t.elapsed();
    total_time += elapsed;
    eprintln!("  [5/5] 插入 1000 音符: {:?}", elapsed);
    let _ = op5;

    eprintln!("  ──────────────────────────");
    eprintln!("  总耗时: {:?}", total_time);
    eprintln!(
        "  峰值 undo 内存: ~{} MB (仅 BitVec)",
        count / 8 / (1024 * 1024)
    );
}

// ─── 测试 5: Arc<Vec<Note>> 快照 undo 对比 ────────────────

#[test]
fn bench_snapshot_undo_cost() {
    let count = 16_000_000;
    let notes = generate_notes(count);
    let mut store_v2 = NoteStoreV2::new(notes.clone(), 10);

    eprintln!("\n═════ Arc<Vec<Note>> 快照 undo: {} 音符 ═════", count);
    let data_mb = count * 16 / (1024 * 1024);
    eprintln!("  单次快照: {} MB", data_mb);

    // 模拟 5 次修改 + undo
    for i in 0..5 {
        store_v2.checkpoint();
        let mut new_notes = (*store_v2.notes).clone();
        // 修改 50% 音符
        for note in new_notes.iter_mut().step_by(2) {
            note.tick += 10.0;
        }
        let t = Instant::now();
        store_v2.commit(new_notes);
        let commit_time = t.elapsed();
        eprintln!("  commit #{}: {:?}", i, commit_time);
    }

    eprintln!("  5 次 undo 快照: {} MB", data_mb * 5);
    eprintln!("  10 次 undo 快照: {} MB", data_mb * 10);

    // 对比可逆操作的内存
    let bitvec_mb = count / 8 / (1024 * 1024);
    eprintln!("  可逆操作 5 次 undo: {} MB (5 × BitVec)", bitvec_mb * 5);
    eprintln!("  内存节省: {:.1}x", (data_mb as f64) / (bitvec_mb as f64));

    drop(store_v2);
}

// ─── 测试 6: 100M 音符外推 ─────────────────────────────────

#[test]
fn bench_100m_extrapolation() {
    // 用 16M 数据外推 100M 性能
    let count = 16_000_000;
    eprintln!("\n═════ 100M 音符外推分析 (16M 实测) ═════");

    let notes = generate_notes(count);
    let selected = generate_selection_50pct(count);

    // 修改 50% 的耗时
    let mut store = NoteStoreV1::new(notes.clone());
    let t = Instant::now();
    let op = store.batch_move(&selected, 10.0, 3);
    let modify_time = t.elapsed();
    let _ = op;

    let factor_100m = 100_000_000.0 / count as f64;
    eprintln!("  16M 50% 修改: {:?}", modify_time);
    eprintln!(
        "  100M 50% 修改预估: {:?}",
        std::time::Duration::from_secs_f64(modify_time.as_secs_f64() * factor_100m)
    );
    eprintln!("  100M 数据内存: {} MB", (100_000_000 * 16) / (1024 * 1024));
    eprintln!(
        "  100M undo 内存 (BitVec): {} MB",
        (100_000_000 / 8) / (1024 * 1024)
    );
    eprintln!(
        "  100M undo 内存 (快照): {} MB",
        (100_000_000 * 16) / (1024 * 1024)
    );
    eprintln!("  建议: 100M 时使用可逆操作, undo 仅 12 MB");
}
