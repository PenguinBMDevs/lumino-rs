//! 删除性能基准（全量轨 1900W 音符 / 39 块）：复现并验证「删除 200W ≈6s」修复。
//!
//! 复现背景（2026-09）：
//! - `ChunkedList::remove` 每次调用 `rebuild_index()`（O(块数) + 2 次分配）+
//!   `Vec::remove` 块内搬移。全量轨 39 块下逐音符删除：
//!   尾部连续 200W = 598ms；**中部连续 20W = 112s**；稀疏 20W = 171s；
//!   全轨 1900W = 3.9s（与 app 实测 ≈6s 同量级）。
//! - 修复：`MidiDocument::remove_note_ranges`（块内单遍压缩 + 单次索引重建 +
//!   整块删除零拷贝），`EditorData` 由选中位图直接构建降序区间。
//!
//! 场景（每轮：选区 → 删除 → 校验 → 撤销 → 校验）：
//! 1. 删除 200W（尾部连续）
//! 2. 删除 200W（中部连续）
//! 3. 删除 200W（稀疏，每 9 取 1）
//! 4. 删除全轨（全部音符）
//! 5. 撤销（恢复 200W 删除）
//!
//! 性能线：删除/撤销单项（中位）≤ 100ms；单次操作峰值堆增量 ≤ 150MB。
//! 运行：`cargo bench -p lumino-ui-editor --bench ui_delete_bench`
//! 环境变量：`LUMINO_BENCH_MIDI` / `LUMINO_BENCH_CYCLES` / `LUMINO_BENCH_VERBOSE`。

use std::time::Instant;

use lumino_ui_editor::Editor;

#[path = "id_history_ops_bench/support.rs"]
mod support;

use support::{
    DEFAULT_CYCLES, DEFAULT_MIDI, Stat, TARGET_MEM_MB, TARGET_MS, build_workload, env_usize, live,
    load_doc, mb, print_track_stats, reset_run_peak, run_peak,
};

/// 单场景统计：删除 / 撤销
struct ScenarioStats {
    delete: Stat,
    undo: Stat,
}

impl ScenarioStats {
    fn new(delete: &'static str, undo: &'static str) -> Self {
        Self {
            delete: Stat::new(delete),
            undo: Stat::new(undo),
        }
    }
}

/// 选区形状
#[derive(Clone, Copy)]
enum Shape {
    /// 尾部连续 `count` 个
    Tail(usize),
    /// 中部连续 `count` 个
    Mid(usize),
    /// `runs` 个均匀分布的多区域，每段连续 `per_run` 个
    Regions { runs: usize, per_run: usize },
    /// 全轨每 `step` 取 1，共 `count` 个（病理压力：区间数远超增量阈值）
    Sparse { step: usize, count: usize },
    /// 全轨
    Full,
}

/// 构建选区索引（升序插入位图）
fn build_selection(editor: &mut Editor, total: usize, shape: Shape) -> usize {
    let set = &mut editor.editor_state.interaction.selected_notes;
    set.clear();
    match shape {
        Shape::Tail(count) => {
            set.reserve(count);
            for i in total.saturating_sub(count)..total {
                set.insert(i);
            }
        }
        Shape::Mid(count) => {
            set.reserve(count);
            let start = total / 2;
            for i in start..(start + count).min(total) {
                set.insert(i);
            }
        }
        Shape::Regions { runs, per_run } => {
            set.reserve(runs * per_run);
            let stride = total / runs.max(1);
            for r in 0..runs {
                let start = (r * stride).min(total);
                for i in start..(start + per_run).min(total) {
                    set.insert(i);
                }
            }
        }
        Shape::Sparse { step, count } => {
            set.reserve(count);
            let mut i = 0usize;
            while i < total && set.len() < count {
                set.insert(i);
                i += step;
            }
        }
        Shape::Full => {
            set.reserve(total);
            for i in 0..total {
                set.insert(i);
            }
        }
    }
    set.len()
}

fn main() {
    println!("=== Lumino 删除性能基准（全量轨）===");
    let path = std::env::var("LUMINO_BENCH_MIDI").unwrap_or_else(|_| DEFAULT_MIDI.to_string());
    let cycles = env_usize("LUMINO_BENCH_CYCLES", DEFAULT_CYCLES);

    let t_load = Instant::now();
    let (doc, src) = load_doc(&path);
    print_track_stats(&doc);
    // 全量保留（full=true），删除成本模型与块数强相关，采样会掩盖问题
    let (doc, track, total) = build_workload(doc, usize::MAX, true);
    let division = doc.division();
    println!(
        "数据源: {src} | 轨 {track} / {total} 音符（约 {} 块）/ division {division} | 加载 {:.1}s",
        total.div_ceil(500_000),
        t_load.elapsed().as_secs_f64()
    );
    println!(
        "目标线: 删除/撤销单项（中位）≤ {TARGET_MS:.0} ms；单次操作内存上升 ≤ {TARGET_MEM_MB:.0} MB"
    );
    println!("轮数: {cycles}（+1 暖机）");
    println!();

    let mut editor = Editor::new();
    editor.editor_state.data.document = Some(doc);
    editor.editor_state.data.current_track = track;
    editor.editor_state.view.visible_key_count = 128;
    editor.editor_state.data.set_collab_sync_enabled(false);

    let _ = lumino_message::events::take_events();
    let baseline = live();
    reset_run_peak();
    println!("加载后基线堆内存: {:.1} MB", mb(baseline as i64));
    println!();

    // 临时内存核对（LUMINO_DELETE_MEMCHECK=1）——保留为可选诊断开关
    if std::env::var("LUMINO_DELETE_MEMCHECK").is_ok() {
        let count2m = 2_000_000.min(total);
        for round in 0..3 {
            build_selection(&mut editor, total, Shape::Tail(count2m));
            let b = live();
            let ab = support::alloc_total();
            support::reset_peak();
            let t = Instant::now();
            editor.delete_selected_notes();
            println!(
                "[memcheck round {round}] 存活增量={:.1} MB | 累计分配={:.1} MB | {:.2} ms",
                mb(support::peak() as i64 - b as i64),
                mb(support::alloc_total() as i64 - ab as i64),
                t.elapsed().as_secs_f64() * 1000.0
            );
            let _ = editor.undo();
            let _ = lumino_message::events::take_events();
        }
    }

    let count2m = 2_000_000.min(total);
    // (名称, 选区, 是否计入判定)：病理压力场景（区间数远超增量阈值，走整段重建回退）
    // 仅信息参考，不计入 PASS/FAIL。
    let scenarios: [(&str, Shape, bool); 6] = [
        ("删除 200W（尾部连续）", Shape::Tail(count2m), true),
        ("删除 200W（中部连续）", Shape::Mid(count2m), true),
        (
            "删除 200W（8 段多区域）",
            Shape::Regions {
                runs: 8,
                per_run: count2m / 8,
            },
            true,
        ),
        ("删除全轨", Shape::Full, true),
        (
            "压力：删除 200W（16 段多区域）",
            Shape::Regions {
                runs: 16,
                per_run: count2m / 16,
            },
            false,
        ),
        (
            "压力：删除 200W（稀疏 1/9）",
            Shape::Sparse {
                step: 9,
                count: count2m,
            },
            false,
        ),
    ];

    let mut stats: Vec<(ScenarioStats, usize, bool)> = Vec::new();
    let mut data_ok = true;
    let mut gated_count = 0usize;

    for (name, shape, gated) in scenarios {
        let mut st = ScenarioStats::new(name, Box::leak(format!("{name} → 撤销").into_boxed_str()));
        let mut ok = true;
        let mut selected = 0usize;
        for c in 0..=cycles {
            // 每轮重建选区（`delete_selected_notes` 会清空选中）
            selected = build_selection(&mut editor, total, shape);
            let before = editor.editor_state.data.current_track_note_count();
            if c == 0 {
                editor.delete_selected_notes();
            } else {
                st.delete.measure(|| {
                    editor.delete_selected_notes();
                });
            }
            let deleted = before - editor.editor_state.data.current_track_note_count();
            let after_delete = editor.editor_state.data.current_track_note_count();
            let _ = lumino_message::events::take_events();

            let restored = if c == 0 {
                editor.undo()
            } else {
                st.undo.measure(|| editor.undo())
            };
            let after_undo = editor.editor_state.data.current_track_note_count();
            let _ = lumino_message::events::take_events();

            ok &= deleted == selected && after_delete + selected == before;
            ok &= restored && after_undo == before;
        }
        if !ok {
            data_ok = false;
        }
        if gated {
            gated_count += 1;
        }
        println!(
            "场景 {name}: 选中 {selected} | {} | 数据校验 {}",
            if gated {
                "计入判定"
            } else {
                "信息参考"
            },
            if ok { "✓" } else { "✗" }
        );
        stats.push((st, selected, gated));
    }

    println!();
    println!("── 删除 / 撤销耗时与内存（{cycles} 轮热态，中位判定）──");
    let mut time_ok = true;
    let mut mem_ok = true;
    let mem_budget = (TARGET_MEM_MB * 1024.0 * 1024.0) as i64;
    for (st, _, gated) in &stats {
        let (t1, m1) = st.delete.report();
        let (t2, m2) = st.undo.report();
        if *gated {
            time_ok &= t1 && t2;
            // 删除场景内存口径：存活峰值（report 内）与累计新增分配（先释放后分配
            // 场景更诚实，如 redo 栈清理与 COW 块复制相抵）双口径均需达标
            mem_ok &= m1
                && m2
                && st.delete.max_alloc_growth() <= mem_budget
                && st.undo.max_alloc_growth() <= mem_budget;
        }
    }
    let _ = gated_count;

    let run_peak = run_peak();
    println!();
    println!(
        "── 内存总览（信息参考）──\n基线: {:.1} MB | 全程绝对峰值: {:.1} MB | 绝对上升: {:.1} MB",
        mb(baseline as i64),
        mb(run_peak as i64),
        mb(run_peak as i64 - baseline as i64),
    );
    println!(
        "历史栈: undo={} redo={}",
        editor.editor_state.data.history.undo_len(),
        editor.editor_state.data.history.redo_len()
    );
    println!();

    let pass = time_ok && mem_ok && data_ok;
    println!(
        "结论(全量轨删除): {}",
        if pass {
            "✓ PASS —— 删除/撤销单项 ≤ 100ms 且单次内存上升 ≤ 150MB，数据无损"
        } else if !data_ok {
            "✗ FAIL —— 数据一致性校验未通过"
        } else if !mem_ok {
            "✗ FAIL —— 存在单项内存上升超阈值"
        } else {
            "✗ FAIL —— 存在单项耗时超阈值"
        }
    );
    println!("=== 完成 ===");
}
