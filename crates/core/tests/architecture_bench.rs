//! 新架构候选方案 release mode 基准测试
//!
//! 目标：验证 Vec<Note> 直接存储 + 原地修改是否能达到：
//!   - 50% 选中提交: <100ms (16M → 8M 修改)
//!   - 全选提交: <500ms
//!   - 冲刺目标: <20ms
//!   - 内存增量: <80MB
//!
//! 运行：cargo test --release -p lumino-core --test architecture_bench -- --nocapture

use bit_vec::BitVec;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
#[repr(C)]  // 确保内存布局紧凑，利于 SIMD
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

// ─── 方案 A: 原地并行修改（Vec<Note> 直接存储，不经过 im::Vector） ──

fn in_place_parallel_modify(
    notes: &mut Vec<Note>,
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
    num_threads: usize,
) -> usize {
    let chunk_size = notes.len().div_ceil(num_threads);

    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(num_threads);
        for (chunk_idx, chunk) in notes.chunks_mut(chunk_size).enumerate() {
            let start = chunk_idx * chunk_size;
            handles.push(s.spawn(move || {
                let mut modified = 0;
                for (local_i, note) in chunk.iter_mut().enumerate() {
                    let global_i = start + local_i;
                    if global_i >= selected.len() || !selected[global_i] {
                        continue;
                    }
                    note.tick = (note.tick + delta_tick).max(0.0);
                    note.key = (note.key as i32 + delta_key as i32).clamp(0, 127) as u16;
                    modified += 1;
                }
                modified
            }));
        }
        let mut total = 0;
        for h in handles {
            total += h.join().unwrap();
        }
        total
    })
}

// ─── 方案 B: 原地并行 + SIMD 友好的批量修改 ──

/// 分两阶段：先扫 BitVec 收集选中索引，再批量修改（更优缓存局部性）
fn in_place_batched_modify(
    notes: &mut [Note],
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
    num_threads: usize,
) -> usize {
    let chunk_size = notes.len().div_ceil(num_threads);
    let notes_len = notes.len();

    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(num_threads);
        for (chunk_idx, chunk) in notes.chunks_mut(chunk_size).enumerate() {
            let start = chunk_idx * chunk_size;
            let end = (start + chunk.len()).min(notes_len);

            handles.push(s.spawn(move || {
                let mut modified = 0;
                // 阶段 1: 收集本 chunk 中选中的索引
                let mut indices: Vec<usize> = Vec::with_capacity(chunk.len() / 2);
                for i in start..end {
                    if i < selected.len() && selected[i] {
                        indices.push(i - start);
                    }
                }
                // 阶段 2: 批量修改（连续内存访问，更优缓存）
                for &local_i in &indices {
                    let note = &mut chunk[local_i];
                    note.tick = (note.tick + delta_tick).max(0.0);
                    note.key = (note.key as i32 + delta_key as i32).clamp(0, 127) as u16;
                    modified += 1;
                }
                modified
            }));
        }
        let mut total = 0;
        for h in handles {
            total += h.join().unwrap();
        }
        total
    })
}

// ─── 测试 1: 大小核架构 vs 均匀分片 ──

#[test]
fn bench_release_in_place_parallel() {
    let note_counts = [5_000_000, 10_000_000, 16_000_000];
    let thread_counts = [4, 8, 16];

    for &count in &note_counts {
        eprintln!("\n═════ {} 音符, 50% 选中 ═════", count);
        let notes = generate_notes(count);
        let selected = generate_selection_50pct(count);

        for &threads in &thread_counts {
            let mut clone = notes.clone();
            let start = Instant::now();
            let modified = in_place_parallel_modify(&mut clone, &selected, 10.0, 3, threads);
            let elapsed = start.elapsed();
            let rate = if elapsed.as_secs_f64() > 0.0 {
                modified as f64 / elapsed.as_secs_f64()
            } else { 0.0 };
            eprintln!("  [{}线程] {:?}, 修改: {} ({:.0}M/s), 内存: 0 MB (原地)",
                threads, elapsed, modified, rate / 1_000_000.0);
        }
    }
}

#[test]
fn bench_release_full_selection() {
    let note_counts = [5_000_000, 10_000_000, 16_000_000];

    for &count in &note_counts {
        eprintln!("\n═════ {} 音符, 全选 ═════", count);
        let mut notes = generate_notes(count);
        let selected = generate_selection_full(count);

        let start = Instant::now();
        let modified = in_place_parallel_modify(&mut notes, &selected, 10.0, 3, 8);
        let elapsed = start.elapsed();
        let rate = if elapsed.as_secs_f64() > 0.0 {
            modified as f64 / elapsed.as_secs_f64()
        } else { 0.0 };
        eprintln!("  [8线程] {:?}, 修改: {} ({:.0}M/s), 内存: 0 MB (原地)",
            elapsed, modified, rate / 1_000_000.0);

        // 检查是否达到目标
        let target = 500_000u64; // 500ms
        if elapsed.as_micros() as u64 <= target {
            eprintln!("  ✓ 达到目标: <500ms");
        } else {
            eprintln!("  ✗ 未达到目标: {:.1}ms > 500ms", elapsed.as_micros() as f64 / 1000.0);
        }
    }
}

#[test]
fn bench_release_batched_vs_direct() {
    // 对比直接遍历 vs 先收集索引再批量修改
    let count = 10_000_000;
    let notes = generate_notes(count);
    let selected = generate_selection_50pct(count);

    eprintln!("\n═════ 分阶段 vs 直接遍历: {} 音符, 50% 选中 ═════", count);

    // 方案 A: 直接遍历
    let mut clone_a = notes.clone();
    let start = Instant::now();
    let modified_a = in_place_parallel_modify(&mut clone_a, &selected, 10.0, 3, 8);
    let elapsed_a = start.elapsed();
    eprintln!("  [直接遍历] {:?}, 修改: {}", elapsed_a, modified_a);

    // 方案 B: 先收集索引再批量修改
    let mut clone_b = notes.clone();
    let start = Instant::now();
    let modified_b = in_place_batched_modify(&mut clone_b, &selected, 10.0, 3, 8);
    let elapsed_b = start.elapsed();
    eprintln!("  [先收集再批量] {:?}, 修改: {}", elapsed_b, modified_b);

    assert_eq!(modified_a, modified_b);
}

#[test]
fn bench_release_track_switch_cost() {
    // 模拟切换到其他音轨再切回的开销（Vec<Note> 需要克隆）
    let count = 16_000_000;
    let notes = generate_notes(count);

    eprintln!("\n═════ 音轨切换开销: {} 音符 ═════", count);
    eprintln!("  数据本身: {} MB", count * 16 / (1024 * 1024));

    // 模拟 Arc<Vec<Note>> 存储音轨副本
    use std::sync::Arc;
    let arc_notes = Arc::new(notes);

    // Arc::clone (O(1))
    let start = Instant::now();
    let _clone = Arc::clone(&arc_notes);
    let arc_clone_elapsed = start.elapsed();
    eprintln!("  Arc<Vec<Note>>::clone(): {:?} (O(1))", arc_clone_elapsed);
    drop(_clone);

    // Arc::make_mut (全量 memcpy)
    let start = Instant::now();
    let _mutable = (*arc_notes).clone();
    let clone_elapsed = start.elapsed();
    let mem = count * 16 / (1024 * 1024);
    eprintln!("  Vec<Note>::clone() (全量memcpy): {:?}, {} MB", clone_elapsed, mem);

    // 内存估算
    let data_mb = count * 16 / (1024 * 1024);
    eprintln!("\n  音轨切换峰值内存: {} MB (Arc 共享, 无克隆)", data_mb);
    eprintln!("  音轨切换 + 修改峰值内存: {} MB (修改时make_mut)", data_mb + data_mb);
}

#[test]
fn bench_release_undo_snapshot_cost() {
    // 评估 undo 快照的内存和速度
    let count = 16_000_000;
    let notes = generate_notes(count);

    eprintln!("\n═════ Undo 快照开销: {} 音符 ═════", count);
    let data_mb = count * 16 / (1024 * 1024);
    eprintln!("  单次快照: {} MB", data_mb);

    // 克隆时间
    let start = Instant::now();
    let snapshot = notes.clone();
    let elapsed = start.elapsed();
    eprintln!("  Vec<Note>::clone(): {:?}", elapsed);

    // 模拟 undo 链: 保留 5 个快照
    let _snapshots: Vec<Vec<Note>> = (0..5).map(|_| snapshot.clone()).collect();
    let total_mb = 5 * data_mb;
    eprintln!("  5 个 undo 快照: {} MB", total_mb);
    eprintln!("  建议: 只保留 1 个快照 + 增量 delta, 或使用 MoveOp 替代");

    // 方案对比: MoveOp 内存
    let selected_count = count / 2;
    let move_op_mb = (selected_count * (8 + 4 + 2)) / (1024 * 1024);
    eprintln!("  MoveOp 方案 (50%选中): ~{} MB (仅 original_ticks/keys)", move_op_mb);
}