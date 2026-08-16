//! 1600W 音符「编辑中间插入 4s + 内存 2-3G」修复回测基准
//!
//! 背景：插入音符后两处 O(N)/O(N log N) 全量路径导致 4s：
//! 1. `ensure_spatial_index` 全量重建（collect NoteRef + sort + 递归建树）
//! 2. 渲染收集 `collect_via_linear_scan` 全量线性扫描
//!
//! 修复：均改为 ChunkedList 二分窗口查询（`window_range` + `iter_window`）。
//! 本基准量化修复前后差异。
//!
//! 运行：cargo test -p lumino-note-core --test insert_perf_backtest -- --ignored --nocapture

use lumino_midi_model::chunked_list::{ChunkedList, EVENT_CHUNK_CAPACITY};
use lumino_midi_model::note_event::NoteEvent;
use lumino_note_core::NoteRef;
use lumino_note_core::spatial_index::NoteSpatialIndex;
use std::time::Instant;

/// 视口窗口 lookback（与 ui-editor 的 NOTES_WINDOW_LOOKBACK 一致）
const WINDOW_LOOKBACK: u32 = 1_000_000;

fn build(count: usize) -> ChunkedList<NoteEvent> {
    let t0 = Instant::now();
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        // 均匀铺满 [0, 2*count]，"中间"落在中部块
        let t = (i as u64 * 2) as u32;
        v.push(NoteEvent::new(t, t + 480, 60, 100, 0));
    }
    let list = ChunkedList::from_sorted(v);
    println!(
        "  构建 {} 事件（{} 块，容量 {}）：{:.2}s",
        count,
        list.chunk_count(),
        EVENT_CHUNK_CAPACITY,
        t0.elapsed().as_secs_f64()
    );
    list
}

#[test]
#[ignore]
fn bench_spatial_rebuild_vs_window_scan() {
    const N: usize = 15_900_000;
    let list = build(N);

    // ── 修复前：空间索引全量重建（插入后 hit_test 路径）──
    let t0 = Instant::now();
    let mut refs: Vec<NoteRef> = Vec::with_capacity(N);
    for (i, n) in list.iter().enumerate() {
        refs.push(NoteRef {
            tick: n.start_tick as f32,
            key: n.key as u16,
            length: (n.end_tick - n.start_tick) as f32,
            index: i,
        });
    }
    let collect_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let t1 = Instant::now();
    let _index = NoteSpatialIndex::from_note_refs(&refs);
    let build_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let peak_mb = (refs.len() * std::mem::size_of::<NoteRef>()) as f64 / 1024.0 / 1024.0;
    println!(
        "[修复前] 空间索引重建：collect {:.0}ms + sort/build {:.0}ms = {:.0}ms（NoteRef 峰值 {:.0}MB）",
        collect_ms,
        build_ms,
        collect_ms + build_ms,
        peak_mb
    );
    drop(refs);
    drop(_index);

    // ── 修复后：窗口扫描（视口 5 万 tick ≈ 中部，含 lookback；上界 +1 保证
    //    start_tick == 视口终点 的音符也进入窗口，与全扫条件一致）──
    let mid_tick = (N as u32 / 2) & !1;
    let t0 = Instant::now();
    let (lo, hi) = list.window_range(mid_tick, mid_tick + 50_000 + 1, WINDOW_LOOKBACK);
    let mut count = 0usize;
    for (_, note) in list.iter_window(lo, hi) {
        // 与全扫完全一致的过滤条件（窗口只做剪枝）
        if note.key as u16 == 60
            && note.end_tick as f32 >= mid_tick as f32
            && note.start_tick as f32 <= (mid_tick + 50_000) as f32
        {
            count += 1;
        }
    }
    let window_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "[修复后] 窗口扫描（视口 5 万 tick）：{:.3}ms（命中 {count}，扫描 {}-{} 全局区间）",
        window_ms, lo, hi
    );

    // ── 修复前：渲染收集全量线性扫描（每次编辑后一帧）──
    let t0 = Instant::now();
    let mut linear_count = 0usize;
    for note in list.iter() {
        if note.key as u16 == 60
            && note.end_tick as f32 >= mid_tick as f32
            && note.start_tick as f32 <= (mid_tick + 50_000) as f32
        {
            linear_count += 1;
        }
    }
    let linear_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "[修复前] 渲染收集全量线性扫描：{:.0}ms（命中 {linear_count}）",
        linear_ms
    );

    assert_eq!(count, linear_count, "窗口扫描与全量扫描结果必须一致");
    println!(
        "⇒ 窗口扫描加速比：全扫 {linear_ms:.0}ms / 窗口 {window_ms:.3}ms ≈ {:.0} 倍",
        linear_ms / window_ms
    );
}

#[test]
#[ignore]
fn bench_insert_then_query_cycle() {
    // 模拟「插入一个音符 → 下次点击 hit_test 窗口查询」的完整循环
    const N: usize = 15_900_000;
    let mut list = build(N);
    let snapshot = list.clone(); // 模拟 push_history 快照（Arc 浅拷贝）

    let mut total_insert = std::time::Duration::ZERO;
    let mut total_query = std::time::Duration::ZERO;
    let cycles = 200;
    for i in 0..cycles {
        let t = (N as u32 / 2) & !1;
        let t0 = Instant::now();
        list.insert(NoteEvent::new(t + i as u32, t + i as u32 + 480, 64, 100, 0));
        total_insert += t0.elapsed();

        // hit_test 窗口查询（含 lookback 跨入）
        let t0 = Instant::now();
        let (lo, hi) = list.window_range(t + i as u32, t + i as u32 + 1, WINDOW_LOOKBACK);
        let mut hits = 0usize;
        for (_, note) in list.iter_window(lo, hi) {
            if note.key as u16 == 64
                && (t + i as u32) as f32 >= note.start_tick as f32
                && (t + i as u32) as f32 <= note.end_tick as f32
            {
                hits += 1;
            }
        }
        total_query += t0.elapsed();
        assert!(hits >= 1, "插入的音符必须能命中");
    }
    println!(
        "[循环] 插入+命中查询 × {cycles}：插入 {:.1}ms/次，命中查询 {:.3}ms/次",
        total_insert.as_secs_f64() * 1000.0 / cycles as f64,
        total_query.as_secs_f64() * 1000.0 / cycles as f64
    );
    assert!(!snapshot.is_empty());
}
