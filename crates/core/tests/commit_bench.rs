//! 后台线程数据处理层性能基准测试
//!
//! 目标：验证 1600 万音符场景下，后台线程的拖拽提交能否在合理时间内完成。
//!
//! 测试内容：
//!   1. BitVec 并行选择 vs HashSet 选择（内存/速度对比）
//!   2. 新旧提交方案对比（Vec<Note> 流式 vs im::Vector COW get_mut）
//!   3. 批量 chunk 提交 vs 全量提交
//!   4. 并行 chunk 处理 vs 单线程处理
//!
//! 运行方式：cargo test -p lumino-core --test commit_bench -- --nocapture

use bit_vec::BitVec;
use im::Vector;
use std::time::Instant;

// ─── 测试数据生成 ────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
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

/// 生成 N 个音符，间隔 10 tick
fn generate_notes(count: usize) -> Vec<Note> {
    (0..count)
        .map(|i| Note::new(i as f32 * 10.0, 60 + (i % 24) as u16, 5.0))
        .collect()
}

/// 生成选中位图：50% 选中（偶数索引）
fn generate_selection_50pct(count: usize) -> BitVec {
    let mut bv = BitVec::from_elem(count, false);
    for i in (0..count).step_by(2) {
        bv.set(i, true);
    }
    bv
}

/// 生成全选位图
fn generate_selection_full(count: usize) -> BitVec {
    BitVec::from_elem(count, true)
}

// ─── 方案 1（旧）：HashSet 选择 + move_ops_from_drag_state ─────

/// 收集选中索引到 Vec<usize>（模拟 move_ops_from_drag_state 中的 selected_indices）
fn collect_selected_indices(selected: &BitVec) -> Vec<usize> {
    selected
        .iter()
        .enumerate()
        .filter(|(_, b)| *b)
        .map(|(i, _)| i)
        .collect()
}

/// 构造 MoveOp 并收集 original_ticks/keys（模拟 move_ops_from_drag_state）
fn build_move_ops(
    notes: &[Note],
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
) -> Vec<MoveOp> {
    let indices = collect_selected_indices(selected);
    let mut ops = Vec::new();
    let mut range_start = 0usize;

    while range_start < indices.len() {
        let mut range_end = range_start + 1;
        while range_end < indices.len() && indices[range_end] == indices[range_end - 1] + 1 {
            range_end += 1;
        }

        let range = &indices[range_start..range_end];
        let original_ticks: Vec<f32> = range.iter().map(|&i| notes[i].tick).collect();
        let original_keys: Vec<u16> = range.iter().map(|&i| notes[i].key).collect();

        ops.push(MoveOp {
            track_id: 0,
            range_start: range[0],
            range_end: range[range.len() - 1] + 1,
            original_ticks,
            original_keys,
            delta_tick,
            delta_key,
        });

        range_start = range_end;
    }

    ops
}

#[derive(Clone, Debug)]
struct MoveOp {
    track_id: usize,
    range_start: usize,
    range_end: usize,
    original_ticks: Vec<f32>,
    original_keys: Vec<u16>,
    delta_tick: f32,
    delta_key: i16,
}

/// 旧方案：通过 MoveOp 应用修改
fn apply_move_ops_old(notes: &mut [Note], ops: &[MoveOp]) -> usize {
    let mut modified = 0;
    for op in ops {
        for (j, idx) in (op.range_start..op.range_end).enumerate() {
            if let Some(note) = notes.get_mut(idx) {
                note.tick = op.original_ticks[j] + op.delta_tick;
                note.key = (op.original_keys[j] as i32 + op.delta_key as i32)
                    .max(0)
                    .min(127) as u16;
                modified += 1;
            }
        }
    }
    modified
}

// ─── 方案 2（新）：直接遍历 BitVec 流式提交 ────────────────────

/// 方案 2a：直接遍历 BitVec（流式，无中间 Vec<usize>）
fn apply_bitvec_streaming(
    notes: &mut [Note],
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
) -> usize {
    let mut modified = 0;
    for (i, is_selected) in selected.iter().enumerate() {
        if !is_selected || i >= notes.len() {
            continue;
        }
        if let Some(note) = notes.get_mut(i) {
            let new_tick = (note.tick + delta_tick).max(0.0);
            let new_key = (note.key as i32 + delta_key as i32).clamp(0, 127) as u16;
            if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                note.tick = new_tick;
                note.key = new_key;
                modified += 1;
            }
        }
    }
    modified
}

/// 方案 2b：先收集 Vec<usize> 再遍历（旧方案的核心部分）
fn apply_selected_indices(
    notes: &mut [Note],
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
) -> usize {
    let indices = collect_selected_indices(selected);
    let mut modified = 0;
    for &i in &indices {
        if let Some(note) = notes.get_mut(i) {
            note.tick = (note.tick + delta_tick).max(0.0);
            note.key = (note.key as i32 + delta_key as i32).clamp(0, 127) as u16;
            modified += 1;
        }
    }
    modified
}

/// 方案 2c：并行 chunk 处理（用 std::thread::scope + chunks_mut 安全分片）
fn apply_bitvec_parallel_chunks(
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
            total += h.join().expect("工作线程 join 应成功");
        }
        total
    })
}

// ─── 方案 3：im::Vector COW 提交（模拟真实后台线程） ──────────

/// 方案 3a：im::Vector get_mut 逐个修改（模拟当前后台线程）
fn apply_im_vector_cow(
    notes: &mut Vector<Note>,
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
) -> usize {
    let mut modified = 0;
    for (i, is_selected) in selected.iter().enumerate() {
        if !is_selected || i >= notes.len() {
            continue;
        }
        if let Some(note) = notes.get_mut(i) {
            let new_tick = (note.tick + delta_tick).max(0.0);
            let new_key = (note.key as i32 + delta_key as i32).clamp(0, 127) as u16;
            if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                note.tick = new_tick;
                note.key = new_key;
                modified += 1;
            }
        }
    }
    modified
}

/// 方案 3b：im::Vector → Vec<Note> → 修改 → 转回 im::Vector
fn apply_im_via_vec(
    notes: &Vector<Note>,
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
) -> (Vector<Note>, usize) {
    let mut modified = 0;
    let mut vec_notes: Vec<Note> = notes.iter().cloned().collect();
    for (i, is_selected) in selected.iter().enumerate() {
        if !is_selected || i >= vec_notes.len() {
            continue;
        }
        let note = &mut vec_notes[i];
        note.tick = (note.tick + delta_tick).max(0.0);
        note.key = (note.key as i32 + delta_key as i32).clamp(0, 127) as u16;
        modified += 1;
    }
    let new_notes: Vector<Note> = vec_notes.into_iter().collect();
    (new_notes, modified)
}

/// 方案 3c：并行 Vec<Note> 修改 + 转回 im::Vector
fn apply_im_via_vec_parallel(
    notes: &Vector<Note>,
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
    num_threads: usize,
) -> (Vector<Note>, usize) {
    let mut vec_notes: Vec<Note> = notes.iter().cloned().collect();
    let modified =
        apply_bitvec_parallel_chunks(&mut vec_notes, selected, delta_tick, delta_key, num_threads);
    let new_notes: Vector<Note> = vec_notes.into_iter().collect();
    (new_notes, modified)
}

// ─── 测试用例 ────────────────────────────────────────────────────

#[test]
fn bench_selection_approaches() {
    // 测试 1: 选择方案对比（只构建索引，不修改数据）
    let note_count = 10_000_000;
    eprintln!("\n═══ 选择方案对比: {} 音符, 50% 选中 ═══", note_count);
    let selected = generate_selection_50pct(note_count);

    // 方案 A: 收集 Vec<usize>（旧方案）
    let start = Instant::now();
    let indices = collect_selected_indices(&selected);
    let elapsed = start.elapsed();
    eprintln!(
        "[旧] selected_indices() → Vec<usize>: {:?}, 长度: {}, 内存: ~{} MB",
        elapsed,
        indices.len(),
        indices.len() * 8 / (1024 * 1024)
    );

    // 方案 B: 直接遍历 BitVec（新方案，不构造中间 Vec）
    let start = Instant::now();
    let mut count = 0usize;
    for (_, b) in selected.iter().enumerate() {
        if b {
            count += 1;
        }
    }
    let elapsed = start.elapsed();
    eprintln!(
        "[新] BitVec 直接遍历: {:?}, 计数: {}, 内存: ~{} MB (不变)",
        elapsed,
        count,
        selected.len() / 8 / (1024 * 1024)
    );
}

#[test]
fn bench_commit_approaches_flat_vec() {
    // 测试 2: 提交方案对比（使用 Vec<Note> 模拟）
    let note_count = 10_000_000;
    let notes = generate_notes(note_count);
    let selected = generate_selection_50pct(note_count);

    eprintln!("\n═══ 提交方案对比: {} 音符, 50% 选中 ═══", note_count);

    // 方案 1: 旧方案 - move_ops_from_drag_state + apply
    let mut notes1 = notes.clone();
    let start = Instant::now();
    let ops = build_move_ops(&notes1, &selected, 10.0, 3);
    let build_time = start.elapsed();
    let start_apply = Instant::now();
    let modified1 = apply_move_ops_old(&mut notes1, &ops);
    let apply_time = start_apply.elapsed();
    eprintln!(
        "[旧] MoveOp 构建: {:?}, 应用: {:?}, 总计: {:?}, 修改: {}",
        build_time,
        apply_time,
        build_time + apply_time,
        modified1
    );
    eprintln!(
        "[旧] MoveOp 中间内存: Vec<usize> {} MB + original_ticks/keys {} MB = {} MB",
        (note_count / 2) * 8 / (1024 * 1024),
        (note_count / 2) * (4 + 2) / (1024 * 1024),
        (note_count / 2) * (8 + 4 + 2) / (1024 * 1024)
    );

    // 方案 2a: 新方案 - 直接 BitVec 遍历
    let mut notes2a = notes.clone();
    let start = Instant::now();
    let modified2a = apply_bitvec_streaming(&mut notes2a, &selected, 10.0, 3);
    let elapsed = start.elapsed();
    eprintln!(
        "[新] BitVec 流式: {:?}, 修改: {}, 内存增量: ~0 MB (原地修改)",
        elapsed, modified2a
    );

    // 方案 2b: 先收集 Vec<usize> 再遍历
    let mut notes2b = notes.clone();
    let start = Instant::now();
    let modified2b = apply_selected_indices(&mut notes2b, &selected, 10.0, 3);
    let elapsed = start.elapsed();
    eprintln!(
        "[新] Vec<usize> + 遍历: {:?}, 修改: {}, 内存增量: ~{} MB",
        elapsed,
        modified2b,
        (note_count / 2) * 8 / (1024 * 1024)
    );

    // 方案 2c: 并行 chunk 处理
    let mut notes2c = notes.clone();
    let start = Instant::now();
    let modified2c = apply_bitvec_parallel_chunks(&mut notes2c, &selected, 10.0, 3, 8);
    let elapsed = start.elapsed();
    eprintln!(
        "[新] BitVec 并行 (8线程): {:?}, 修改: {}, 内存增量: ~0 MB (原地修改)",
        elapsed, modified2c
    );
}

#[test]
fn bench_im_vector_cow_overhead() {
    // 测试 3: im::Vector COW 开销对比
    let note_count = 5_000_000;
    eprintln!(
        "\n═══ im::Vector COW 开销: {} 音符, 50% 选中 ═══",
        note_count
    );

    let note_vec = generate_notes(note_count);
    let mut im_notes: Vector<Note> = Vector::from(note_vec.clone());
    let selected = generate_selection_50pct(note_count);

    // 方案 3a: im::Vector get_mut COW（模拟后台线程）
    let start = Instant::now();
    let modified3a = apply_im_vector_cow(&mut im_notes, &selected, 10.0, 3);
    let elapsed = start.elapsed();
    eprintln!(
        "[3a] im::Vector get_mut (COW): {:?}, 修改: {}",
        elapsed, modified3a
    );

    // 方案 3b: im::Vector → Vec → 修改 → 转回
    let im_notes2: Vector<Note> = Vector::from(note_vec.clone());
    let start = Instant::now();
    let (_, modified3b) = apply_im_via_vec(&im_notes2, &selected, 10.0, 3);
    let elapsed = start.elapsed();
    eprintln!(
        "[3b] im::Vector → Vec → 修改 → 转回: {:?}, 修改: {}, 中间 Vec: ~{} MB",
        elapsed,
        modified3b,
        note_count * 16 / (1024 * 1024)
    );

    // 方案 3c: 并行 Vec 修改
    let im_notes3: Vector<Note> = Vector::from(note_vec);
    let start = Instant::now();
    let (_, modified3c) = apply_im_via_vec_parallel(&im_notes3, &selected, 10.0, 3, 8);
    let elapsed = start.elapsed();
    eprintln!(
        "[3c] im::Vector → Vec (并行8线程) → 转回: {:?}, 修改: {}",
        elapsed, modified3c
    );

    // 验证正确性
    assert_eq!(modified3a, modified3b);
    assert_eq!(modified3a, modified3c);
}

#[test]
fn bench_im_vector_full_selection() {
    // 测试 4: 全选情况下的最坏性能
    let note_count = 5_000_000;
    eprintln!("\n═══ im::Vector 全选最坏情况: {} 音符 ═══", note_count);

    let note_vec = generate_notes(note_count);
    let selected = generate_selection_full(note_count);

    // im::Vector COW（全选）
    let mut im_notes: Vector<Note> = Vector::from(note_vec.clone());
    let start = Instant::now();
    let modified = apply_im_vector_cow(&mut im_notes, &selected, 10.0, 3);
    let elapsed = start.elapsed();
    let rate = if elapsed.as_secs_f64() > 0.0 {
        (modified as f64 / elapsed.as_secs_f64()) as u64
    } else {
        0
    };
    eprintln!(
        "[COW] 全选: {:?}, 修改: {}, 速率: {:.2}M/s",
        elapsed,
        modified,
        rate as f64 / 1_000_000.0
    );

    // Vec 方案（全选）
    let im_notes2: Vector<Note> = Vector::from(note_vec.clone());
    let start = Instant::now();
    let (_, modified2) = apply_im_via_vec(&im_notes2, &selected, 10.0, 3);
    let elapsed = start.elapsed();
    let rate2 = if elapsed.as_secs_f64() > 0.0 {
        (modified2 as f64 / elapsed.as_secs_f64()) as u64
    } else {
        0
    };
    eprintln!(
        "[Vec] 全选: {:?}, 修改: {}, 速率: {:.2}M/s",
        elapsed,
        modified2,
        rate2 as f64 / 1_000_000.0
    );

    // 并行 Vec 方案（全选）
    let im_notes3: Vector<Note> = Vector::from(note_vec);
    let start = Instant::now();
    let (_, modified3) = apply_im_via_vec_parallel(&im_notes3, &selected, 10.0, 3, 8);
    let elapsed = start.elapsed();
    let rate3 = if elapsed.as_secs_f64() > 0.0 {
        (modified3 as f64 / elapsed.as_secs_f64()) as u64
    } else {
        0
    };
    eprintln!(
        "[Vec并行8核] 全选: {:?}, 修改: {}, 速率: {:.2}M/s",
        elapsed,
        modified3,
        rate3 as f64 / 1_000_000.0
    );
}

#[test]
fn bench_scalability_analysis() {
    // 测试 5: 可扩展性分析 - 不同数据量下的性能
    let sizes = [1_000_000, 5_000_000, 10_000_000];
    let num_threads = [1, 4, 8, 16];

    for &count in &sizes {
        eprintln!("\n═══ 可扩展性: {} 音符, 50% 选中 ═══", count);
        let note_vec = generate_notes(count);
        let selected = generate_selection_50pct(count);

        let im_notes: Vector<Note> = Vector::from(note_vec.clone());

        for &threads in &num_threads {
            let im_clone = im_notes.clone();
            let selected_clone = selected.clone();

            let start = Instant::now();
            let (_, modified) =
                apply_im_via_vec_parallel(&im_clone, &selected_clone, 10.0, 3, threads);
            let elapsed = start.elapsed();
            let rate = if elapsed.as_secs_f64() > 0.0 {
                (modified as f64 / elapsed.as_secs_f64()) as u64
            } else {
                0
            };
            eprintln!(
                "  {} 线程: {:?}, 修改: {}, {:.2}M/s",
                threads,
                elapsed,
                modified,
                rate as f64 / 1_000_000.0
            );
        }
    }
}

#[test]
fn bench_16m_extrapolation() {
    // 测试 6: 用 5M 数据量外推 16M 性能
    use std::time::Instant;

    let note_count = 5_000_000;
    let note_vec = generate_notes(note_count);
    let _selected = generate_selection_50pct(note_count);

    eprintln!("\n═══ 1600 万外推分析 (5M 实测) ═══");

    // 1. 测量 im::Vector clone 时间
    let im_notes: Vector<Note> = Vector::from(note_vec.clone());
    let start = Instant::now();
    let im_clone = im_notes.clone();
    let clone_time = start.elapsed();
    eprintln!("im::Vector::clone() (O(1) Arc): {:?}", clone_time);
    drop(im_clone);

    // 2. 测量 Vec 克隆时间
    let start = Instant::now();
    let vec_clone: Vec<Note> = note_vec.clone();
    let vec_clone_time = start.elapsed();
    let vec_memory = note_count * 16 / (1024 * 1024);
    eprintln!(
        "Vec<Note>::clone() (O(N) 拷贝): {:?}, 内存: ~{} MB",
        vec_clone_time, vec_memory
    );
    drop(vec_clone);

    // 3. 测量 im::Vector → Vec 转换时间
    let im_notes2: Vector<Note> = Vector::from(note_vec.clone());
    let start = Instant::now();
    let vec_from_im: Vec<Note> = im_notes2.iter().cloned().collect();
    let vec_from_time = start.elapsed();
    eprintln!("im::Vector → Vec<Note> (iter+clone): {:?}", vec_from_time);
    drop(vec_from_im);

    // 4. 测量 Vec → im::Vector 转换时间
    let vec_notes = note_vec.clone();
    let start = Instant::now();
    let im_from_vec: Vector<Note> = vec_notes.into_iter().collect();
    let im_from_time = start.elapsed();
    eprintln!("Vec<Note> → im::Vector (collect): {:?}", im_from_time);
    drop(im_from_vec);

    // 外推结果
    let factor_16m = 16_000_000.0 / note_count as f64;
    eprintln!("\n━━━ 外推到 1600 万 ━━━");
    eprintln!(
        "Vec<Note> 克隆: ~{} MB × {} = {} MB",
        vec_memory,
        factor_16m as usize,
        (vec_memory as f64 * factor_16m) as usize
    );
    eprintln!(
        "Vec 克隆耗时 (预估): {:?} × {:.1} = {:?}",
        vec_clone_time,
        factor_16m,
        std::time::Duration::from_secs_f64(vec_clone_time.as_secs_f64() * factor_16m)
    );
    eprintln!(
        "im::Vector → Vec 耗时 (预估): {:?} × {:.1} = {:?}",
        vec_from_time,
        factor_16m,
        std::time::Duration::from_secs_f64(vec_from_time.as_secs_f64() * factor_16m)
    );
    eprintln!(
        "Vec → im::Vector 耗时 (预估): {:?} × {:.1} = {:?}",
        im_from_time,
        factor_16m,
        std::time::Duration::from_secs_f64(im_from_time.as_secs_f64() * factor_16m)
    );
}

// ─── 新架构：后台线程完整流水线模拟 ────────────────────────────

/// 后台线程 Vec 提交流水线（模拟实际生产代码）
///
/// 模拟后台线程的完整流程：
/// 1. 主线程克隆 im::Vector (Arc O(1))
/// 2. 后台线程: im::Vector → Vec<Note> 转换
/// 3. 后台线程: 并行分块修改
/// 4. 后台线程: Vec<Note> → im::Vector 转换
/// 5. 后台线程: 发送结果回主线程
/// 6. 主线程: 原子交换
fn background_vec_commit_pipeline(
    notes: &Vector<Note>,
    selected: &BitVec,
    delta_tick: f32,
    delta_key: i16,
    num_threads: usize,
) -> (Vector<Note>, usize, std::time::Duration) {
    // 阶段 1: im::Vector → Vec
    let t0 = std::time::Instant::now();
    let mut vec_notes: Vec<Note> = notes.iter().cloned().collect();
    let convert_time = t0.elapsed();

    // 阶段 2: 并行修改
    let t1 = std::time::Instant::now();
    let chunk_size = vec_notes.len().div_ceil(num_threads);

    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(num_threads);
        for (chunk_idx, chunk) in vec_notes.chunks_mut(chunk_size).enumerate() {
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
            total += h.join().expect("工作线程 join 应成功");
        }
        total
    });
    let modify_time = t1.elapsed();

    // 阶段 3: Vec → im::Vector
    let t2 = std::time::Instant::now();
    let new_notes: Vector<Note> = vec_notes.into_iter().collect();
    let rebuild_time = t2.elapsed();

    let total = convert_time + modify_time + rebuild_time;
    (new_notes, 0, total)
}

#[test]
fn bench_vec_commit_pipeline() {
    // 测试完整流水线: 5M 音符, 50% 选中
    let note_count = 5_000_000;
    let note_vec = generate_notes(note_count);
    let selected = generate_selection_50pct(note_count);
    let im_notes: Vector<Note> = Vector::from(note_vec);

    eprintln!("\n═══ 后台线程流水线: {} 音符, 50% 选中 ═══", note_count);

    // 测量各阶段耗时
    let t0 = std::time::Instant::now();
    let mut vec_notes: Vec<Note> = im_notes.iter().cloned().collect();
    let convert_time = t0.elapsed();
    eprintln!(
        "[阶段1] im::Vector → Vec<Note>: {:?}, 内存: ~{} MB",
        convert_time,
        note_count * 16 / (1024 * 1024)
    );

    let t1 = std::time::Instant::now();
    let modified = {
        let chunk_size = vec_notes.len().div_ceil(8);
        let selected_ref = &selected;
        std::thread::scope(|s| {
            let mut handles = Vec::with_capacity(8);
            for (chunk_idx, chunk) in vec_notes.chunks_mut(chunk_size).enumerate() {
                let start = chunk_idx * chunk_size;
                handles.push(s.spawn(move || {
                    let mut m = 0;
                    for (local_i, note) in chunk.iter_mut().enumerate() {
                        let gi = start + local_i;
                        if gi >= selected_ref.len() || !selected_ref[gi] {
                            continue;
                        }
                        note.tick = (note.tick + 10.0).max(0.0);
                        note.key = (note.key as i32 + 3).clamp(0, 127) as u16;
                        m += 1;
                    }
                    m
                }));
            }
            let mut total = 0;
            for h in handles {
                total += h.join().expect("工作线程 join 应成功");
            }
            total
        })
    };
    let modify_time = t1.elapsed();
    eprintln!(
        "[阶段2] 并行修改 (8线程): {:?}, 修改: {}",
        modify_time, modified
    );

    let t2 = std::time::Instant::now();
    let _new_im: Vector<Note> = vec_notes.into_iter().collect();
    let rebuild_time = t2.elapsed();
    eprintln!("[阶段3] Vec<Note> → im::Vector: {:?}", rebuild_time);

    let total = convert_time + modify_time + rebuild_time;
    eprintln!("[总计] 流水线: {:?}", total);
    eprintln!(
        "[外推] 16M 预期: {:?}",
        std::time::Duration::from_secs_f64(total.as_secs_f64() * 3.2)
    );
}

#[test]
fn bench_vec_commit_pipeline_full_selection() {
    // 全选最坏情况: 5M 音符
    let note_count = 5_000_000;
    let note_vec = generate_notes(note_count);
    let selected = generate_selection_full(note_count);
    let im_notes: Vector<Note> = Vector::from(note_vec);

    eprintln!("\n═══ 后台线程流水线 (全选最坏): {} 音符 ═══", note_count);

    let total = background_vec_commit_pipeline(&im_notes, &selected, 10.0, 3, 8);
    let (_, modified, elapsed) = total;
    let rate = modified as f64 / elapsed.as_secs_f64();
    eprintln!(
        "[总计] {:?}, 修改: {}, 速率: {:.2}M/s",
        elapsed,
        modified,
        rate / 1_000_000.0
    );
    eprintln!(
        "[外推] 16M: {:?}",
        std::time::Duration::from_secs_f64(elapsed.as_secs_f64() * 3.2)
    );
}

#[test]
fn bench_arc_vec_alternative() {
    // 测试使用 Arc<Vec<Note>> 替代 im::Vector 的可能性
    // 如果底层存储改为 Arc<Vec<Note>>，commit 可以零拷贝
    let note_count = 5_000_000;
    let note_vec = generate_notes(note_count);
    let selected = generate_selection_50pct(note_count);

    eprintln!(
        "\n═══ Arc<Vec<Note>> 替代方案: {} 音符, 50% 选中 ═══",
        note_count
    );

    // 当前: im::Vector，需要完整转换
    let im_notes: Vector<Note> = Vector::from(note_vec.clone());
    let t0 = std::time::Instant::now();
    let _vec: Vec<Note> = im_notes.iter().cloned().collect();
    let im_to_vec = t0.elapsed();
    eprintln!("[im] im::Vector → Vec: {:?}", im_to_vec);

    let t1 = std::time::Instant::now();
    let _im2: Vector<Note> = _vec.into_iter().collect();
    let vec_to_im = t1.elapsed();
    eprintln!("[im] Vec → im::Vector: {:?}", vec_to_im);

    // 替代: Arc<Vec<Note>>，clone 是 O(1) Arc bump
    use std::sync::Arc;
    let arc_vec = Arc::new(note_vec);
    let t2 = std::time::Instant::now();
    let cloned = Arc::clone(&arc_vec);
    let arc_clone = t2.elapsed();
    eprintln!("[Arc] Arc<Vec<Note>>::clone(): {:?}", arc_clone);
    drop(cloned);

    // Arc::make_mut 触发 COW，但只需要一次 memcpy，不是 N 次树遍历
    let t3 = std::time::Instant::now();
    let mut mutable = (*arc_vec).clone();
    let arc_cow = t3.elapsed();
    eprintln!("[Arc] Arc::make_mut (全量 memcpy): {:?}", arc_cow);

    // 修改 Arc<Vec<Note>> 中的元素
    let t4 = std::time::Instant::now();
    for (i, note) in mutable.iter_mut().enumerate() {
        if i >= selected.len() || !selected[i] {
            continue;
        }
        note.tick = (note.tick + 10.0).max(0.0);
        note.key = (note.key as i32 + 3).clamp(0, 127) as u16;
    }
    let arc_modify = t4.elapsed();
    eprintln!("[Arc] 单线程修改: {:?}", arc_modify);

    // 并行修改 Arc<Vec<Note>>
    let mut mutable2 = (*arc_vec).clone();
    let t5 = std::time::Instant::now();
    let chunk_size = mutable2.len().div_ceil(8);
    let selected_ref = &selected;
    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(8);
        for (chunk_idx, chunk) in mutable2.chunks_mut(chunk_size).enumerate() {
            let start = chunk_idx * chunk_size;
            handles.push(s.spawn(move || {
                for (local_i, note) in chunk.iter_mut().enumerate() {
                    let gi = start + local_i;
                    if gi >= selected_ref.len() || !selected_ref[gi] {
                        continue;
                    }
                    note.tick = (note.tick + 10.0).max(0.0);
                    note.key = (note.key as i32 + 3).clamp(0, 127) as u16;
                }
            }));
        }
        for h in handles {
            h.join().expect("工作线程 join 应成功");
        }
    });
    let arc_parallel_modify = t5.elapsed();
    eprintln!("[Arc] 并行8线程修改: {:?}", arc_parallel_modify);
    drop(mutable);
    drop(mutable2);

    let im_total = im_to_vec + vec_to_im;
    let arc_total = arc_cow + arc_parallel_modify;
    eprintln!(
        "\n[对比] im::Vector 往返: {:?} | Arc<Vec> 修改: {:?} | 速度比: {:.1}x",
        im_total,
        arc_total,
        im_total.as_secs_f64() / arc_total.as_secs_f64().max(1e-9)
    );
    eprintln!("[对比] Arc<Vec> 无需 im::Vector 转换，直接 clone 修改");
    eprintln!("[对比] 但 Arc<Vec> 不支持持久化/undo 快照，需额外方案");
}
