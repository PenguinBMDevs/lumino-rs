//! ChunkedList 中间插入性能复现基准（1590W 音符场景）
//!
//! 背景：1600W 音符工程"编辑中间插入"单事件 4s、内存增量 2-3G。
//! 目标：测量 ChunkedList::insert 在最坏场景（中间块 + 快照持 Arc）的真实成本。
//!
//! 运行：cargo test -p lumino-midi-model --test insert_perf -- --ignored --nocapture

use lumino_midi_model::chunked_list::ChunkedList;
use lumino_midi_model::note_event::NoteEvent;
use std::time::Instant;

fn build(count: usize) -> ChunkedList<NoteEvent> {
    let t0 = Instant::now();
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        // 均匀铺满 [0, 2*count]，确保"中间插入"落在中部块
        let t = (i as u64 * 2) as u32;
        v.push(NoteEvent::new(t, t + 1, 60, 100, 0));
    }
    let list = ChunkedList::from_sorted(v);
    println!(
        "  构建 {} 事件（Vec + from_sorted）：{:.2}s",
        count,
        t0.elapsed().as_secs_f64()
    );
    list
}

fn measure_insert(label: &str, list: &mut ChunkedList<NoteEvent>, tick_floor: u32) {
    let t0 = Instant::now();
    let n = 200;
    for i in 0..n {
        let t = tick_floor + i as u32;
        list.insert(NoteEvent::new(t, t + 1, 64, 100, 0));
    }
    let elapsed = t0.elapsed();
    println!(
        "  [{label}] 插入 {n} 事件 = {:.2}s，单事件 {:.1}ms",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / n as f64
    );
}

#[test]
#[ignore]
fn bench_insert_baseline() {
    let mut list = build(15_900_000);
    let last_half_floor = (list.len() as u32 / 2) & !1;
    measure_insert("中间插入(无快照)", &mut list, last_half_floor);
}

#[test]
#[ignore]
fn bench_insert_with_snapshot() {
    // 模拟 push_history()：快照浅拷贝持有全部块 Arc → insert 时 make_mut 深拷贝
    let mut list = build(15_900_000);
    let snapshot = list.clone(); // O(块数) 浅拷贝
    let mid = (list.len() as u32 / 2) & !1;
    measure_insert("中间插入(持快照Arc)", &mut list, mid);
    // 防止编译期优化掉快照
    assert!(snapshot.len() > 0);
}

#[test]
#[ignore]
fn bench_append_with_snapshot() {
    let mut list = build(15_900_000);
    let snapshot = list.clone();
    // 尾部插入（落到最后一块，不分裂）
    let tail = (list.len() as u32) & !1;
    measure_insert("尾部插入(有快照Arc)", &mut list, tail);
    std::hint::black_box(&snapshot);
}

#[test]
#[ignore]
fn bench_mem_estimate() {
    // 估算单块 Arc 共享 vs 深拷贝的内存：用 size_of 与构造膨胀说明
    let list = build(100_000);
    let snapshot = list.clone();
    // 若插入触发 make_mut，新块 = 原块复制。这里只测块内插入对象大小
    println!("  NoteEvent size = {}", std::mem::size_of::<NoteEvent>());
    println!(
        "  单块容量 = 500k，单块字节 ≈ {:.1}MB",
        std::mem::size_of::<NoteEvent>() as f64 * 500_000.0 / 1_048_576.0
    );
    drop(snapshot);
}
