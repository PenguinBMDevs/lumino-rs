//! ID 型撤销/重做 + 复制/粘贴 + 选中 连续操作性能基准（200W 音符）
//!
//! 复刻钢琴卷帘编辑器的真实数据操作路径（与 UI 层实现同构）：
//! - **选中**：`Editor::select_all_notes`（`selected_notes` 单一表示 + 边界缓存，
//!   原地复用哈希容量 + 单次轨道扫描）
//! - **复制**：`Editor::build_clipboard_binary`（生产编码路径：紧凑二进制、流式编码、
//!   全选快路径）
//! - **粘贴**：与 `Editor::try_paste_from_binary` 同构（流式解码升序 NoteEvent →
//!   **单次** O(N+M) 批量插入 + 历史快照）
//! - **撤销/重做**：`Editor::undo` / `Editor::redo`（历史条目以 note id 引用音符 +
//!   主选择身份重映射；未连接协作时跳过 O(N) 协作对账）
//! - **附加观测**：ID `MoveOp`（ids + 原始位置）前向应用/撤销/重做——非必需操作集，
//!   仅供可见性，不计入判定
//!
//! 性能线（用户给定）：
//! - 必需操作单项 ≤ 100ms（判定取**中位数**，最大/平均仅参考：共享开发机的单轮
//!   最大受系统调度噪声支配）
//! - 单次操作内存峰值堆增量 ≤ 150MB（历史快照等跨操作保留状态另以绝对峰值信息性报道）
//!
//! 数据：`D:\BM-DATA\MIDI File\Toilet Story 6 F2.mid`（52,255,193 音符 / 15 轨 /
//! division 960）。默认取音符最多音轨的 **200W 条均匀采样**构建工作负载，保证 tick 覆盖
//! 全曲；`LUMINO_BENCH_NOTES` 可调规模，`LUMINO_BENCH_FULL=1` 用整份文档压测。
//! 文件缺失时回退合成 200W 数据。
//!
//! 运行：
//! ```bash
//! cargo bench -p lumino-ui-editor --bench id_history_ops_bench
//! ```
//!
//! 内存口径：进程级统计分配器（每次 alloc/dealloc 记账）报道**堆内存**；不含 OS/图形内存。
//! 消息层事件总线（协作广播 emit）不计入各项耗时与单次内存——其消费端在未连接协作时
//! 短路丢弃，属独立消息层而非编辑数据操作。
//!
//! 统计口径：先跑 1 轮**冷启动暖机**（页表/分配器预热，仅参考不计入判定），
//! 随后连续 N 轮（默认 5）热态操作取最大值为判定依据。

use std::env;
use std::time::Instant;

use lumino_ui_editor::Editor;

#[path = "id_history_ops_bench/ops.rs"]
mod ops;
#[path = "id_history_ops_bench/support.rs"]
mod support;

use ops::{Identity, build_move_ops, drain_events, encode_selected, paste_payload, track_identity};
use support::{
    DEFAULT_CYCLES, DEFAULT_MIDI, DEFAULT_NOTES, Stat, TARGET_MEM_MB, TARGET_MS, build_workload,
    env_usize, live, load_doc, mb, print_track_stats, reset_run_peak, run_peak,
};

/// 必需操作集统计
struct RequiredStats {
    select: Stat,
    copy: Stat,
    paste: Stat,
    undo: Stat,
    redo: Stat,
    undo2: Stat,
}

impl RequiredStats {
    fn new() -> Self {
        Self {
            select: Stat::new("选中（全选 200W）"),
            copy: Stat::new("复制（二进制编码）"),
            paste: Stat::new("粘贴（解码+批量插入）"),
            undo: Stat::new("撤销（粘贴回退）"),
            redo: Stat::new("重做（粘贴重放）"),
            undo2: Stat::new("撤销（回到基线）"),
        }
    }
}

/// 附加观测（ID MoveOp 路径）
struct MoveStats {
    apply: Stat,
    undo: Stat,
    redo: Stat,
}

impl MoveStats {
    fn new() -> Self {
        Self {
            apply: Stat::new("移动（ID MoveOp 前向+入栈）"),
            undo: Stat::new("移动撤销（ID 恢复）"),
            redo: Stat::new("移动重做（ID 重放）"),
        }
    }
}

/// 单轮连续操作结果
struct CycleReport {
    after_paste: usize,
    after_undo1: usize,
    after_redo: usize,
    after_undo2: usize,
    selected: usize,
    inserted: usize,
    cycle_ok: bool,
    data_ok: bool,
}

/// 执行一轮连续操作：选中 → 复制 → 粘贴 → 撤销 → 重做 → 撤销（+ ID MoveOp 附加观测）
fn run_cycle(
    editor: &mut Editor,
    required: &mut RequiredStats,
    mv: &mut MoveStats,
    workload: usize,
    identity_before: &Identity,
) -> CycleReport {
    // ── 1. 选中 ──
    required.select.measure(|| editor.select_all_notes());
    drain_events();
    // ── 2. 复制 ──
    let payload = required.copy.measure(|| encode_selected(editor));
    drain_events();
    let selected = editor.selected_notes_count();
    // ── 3. 粘贴 ──
    let inserted = required.paste.measure(|| paste_payload(editor, &payload));
    drain_events();
    // 生产路径：载荷写入系统剪贴板后应用侧不再持有（此处同步释放，避免跨操作保留）
    drop(payload);
    let after_paste = editor.editor_state.data.current_track_notes().len();
    // ── 4. 撤销（粘贴回退）──
    let undo1 = required.undo.measure(|| editor.undo());
    drain_events();
    let after_undo1 = editor.editor_state.data.current_track_notes().len();
    // ── 5. 重做（粘贴重放）──
    let redo1 = required.redo.measure(|| editor.redo());
    drain_events();
    let after_redo = editor.editor_state.data.current_track_notes().len();
    // ── 6. 撤销（回到基线）──
    let undo2 = required.undo2.measure(|| editor.undo());
    drain_events();
    let after_undo2 = editor.editor_state.data.current_track_notes().len();

    let cycle_ok = selected == workload
        && inserted == workload
        && after_paste == workload * 2
        && undo1
        && after_undo1 == workload
        && redo1
        && after_redo == workload * 2
        && undo2
        && after_undo2 == workload;

    // ── 7. 附加观测：ID MoveOp：入栈 + 前向应用 → 撤销 → 重做 → 撤销 ──
    let ops = build_move_ops(editor);
    let ops_for_history = ops.clone();
    mv.apply.measure(|| {
        editor.editor_state.data.apply_move_ops(&ops, false, 255);
        editor.editor_state.data.push_move_op(ops_for_history);
    });
    drain_events();
    let moved_identity = track_identity(editor);
    let mv_undo1 = mv.undo.measure(|| editor.undo());
    drain_events();
    let restored = track_identity(editor);
    let mv_redo = mv.redo.measure(|| editor.redo());
    drain_events();
    let moved_again = track_identity(editor);
    let mv_undo2 = mv.undo.measure(|| editor.undo());
    drain_events();
    let restored_again = track_identity(editor);

    let data_ok = cycle_ok
        && track_identity(editor) == *identity_before
        && mv_undo1
        && mv_redo
        && mv_undo2
        && restored == *identity_before
        && moved_again == moved_identity
        && restored_again == *identity_before;

    CycleReport {
        after_paste,
        after_undo1,
        after_redo,
        after_undo2,
        selected,
        inserted,
        cycle_ok,
        data_ok,
    }
}

fn main() {
    println!("=== Lumino ID 型撤销/重做 + 复制/粘贴 + 选中 连续操作基准 ===");
    let path = env::var("LUMINO_BENCH_MIDI").unwrap_or_else(|_| DEFAULT_MIDI.to_string());
    let target_notes = env_usize("LUMINO_BENCH_NOTES", DEFAULT_NOTES);
    let cycles = env_usize("LUMINO_BENCH_CYCLES", DEFAULT_CYCLES);
    let full = env::var("LUMINO_BENCH_FULL").is_ok();

    let t_load = Instant::now();
    let (doc, src) = load_doc(&path);
    print_track_stats(&doc);
    println!(
        "数据源: {src} | 全量音轨数 {} | 全量音符 {} | 加载 {:.1}s",
        doc.track_count(),
        (0..doc.track_count())
            .map(|t| doc.track_notes(t).len())
            .sum::<usize>(),
        t_load.elapsed().as_secs_f64()
    );

    let (doc, track, workload) = build_workload(doc, target_notes, full);
    let division = doc.division();
    let first_tick = doc.track_notes(track).first().map(|n| n.start_tick);
    let last_tick = doc.track_notes(track).last().map(|n| n.start_tick);
    println!(
        "工作负载: 轨 {track} / {workload} 音符 / division {division} / tick 覆盖 {first_tick:?}..{last_tick:?} | 轮数 {cycles}(+1 暖机)"
    );
    println!(
        "目标线: 必需操作单项（中位）≤ {TARGET_MS:.0} ms；单次操作内存上升 ≤ {TARGET_MEM_MB:.0} MB"
    );
    println!();

    let mut editor = Editor::new();
    editor.editor_state.data.document = Some(doc);
    editor.editor_state.data.current_track = track;
    editor.editor_state.view.visible_key_count = 128;
    // 本地编辑（无协作会话）：关闭协作同步——与生产「未连接协作」路径一致，
    // undo/redo 不做整轨 O(N) id 对账、不构建批量广播载荷。
    editor.editor_state.data.set_collab_sync_enabled(false);

    let baseline = live();
    reset_run_peak();
    println!("加载后基线堆内存: {:.1} MB", mb(baseline as i64));
    println!();

    let identity_before = track_identity(&editor);

    // ── 冷启动暖机一轮（不计入判定，仅参考）──
    let mut cold = RequiredStats::new();
    let mut cold_mv = MoveStats::new();
    let cold_report = run_cycle(
        &mut editor,
        &mut cold,
        &mut cold_mv,
        workload,
        &identity_before,
    );
    println!("── 冷启动首轮（页表/分配器预热；仅参考，不计入判定）──");
    let _ = cold.select.report();
    let _ = cold.copy.report();
    let _ = cold.paste.report();
    println!(
        "冷启动数据校验: {}",
        if cold_report.data_ok { "✓" } else { "✗" }
    );
    println!();

    // ── 热态连续 N 轮（判定依据）──
    let mut required = RequiredStats::new();
    let mut mv = MoveStats::new();
    let mut checksum_ok = true;
    for c in 0..cycles {
        let r = run_cycle(
            &mut editor,
            &mut required,
            &mut mv,
            workload,
            &identity_before,
        );
        checksum_ok &= r.data_ok;
        println!(
            "第 {}/{} 轮: 粘贴后 {} | 撤销后 {} | 重做后 {} | 撤销后 {} | 选中 {} | 粘贴 {} | 数据校验 {}",
            c + 1,
            cycles,
            r.after_paste,
            r.after_undo1,
            r.after_redo,
            r.after_undo2,
            r.selected,
            r.inserted,
            if r.cycle_ok { "✓" } else { "✗" }
        );
    }

    println!();
    println!("── 必需操作耗时 / 内存（{cycles} 轮热态；内存=单次操作峰值堆增量）──");
    let mut time_ok = true;
    let mut mem_ok = true;
    macro_rules! report_gated {
        ($st:expr) => {{
            let (t, m) = $st.report();
            time_ok &= t;
            mem_ok &= m;
        }};
    }
    report_gated!(required.select);
    report_gated!(required.copy);
    report_gated!(required.paste);
    report_gated!(required.undo);
    report_gated!(required.redo);
    report_gated!(required.undo2);

    println!("── 附加观测：ID MoveOp 路径（非必需操作集，仅供参考）──");
    let _ = mv.apply.report();
    let _ = mv.undo.report();
    let _ = mv.redo.report();

    let run_peak = run_peak();
    let mem_growth = run_peak as i64 - baseline as i64;
    println!();
    println!(
        "── 内存总览（信息参考）──\n基线: {:.1} MB | 全程绝对峰值: {:.1} MB | 绝对上升: {:.1} MB（含历史快照/选中容量等跨操作保留，非单次操作预算）",
        mb(baseline as i64),
        mb(run_peak as i64),
        mb(mem_growth),
    );
    println!(
        "历史栈: undo={} redo={} | 数据一致性: {}",
        editor.editor_state.data.history.undo_len(),
        editor.editor_state.data.history.redo_len(),
        if checksum_ok {
            "✓ 全部轮次无损"
        } else {
            "✗ 存在偏差"
        }
    );
    println!();

    let pass = time_ok && mem_ok && checksum_ok;
    println!(
        "结论(200W 音符连续操作): {}",
        if pass {
            "✓ PASS —— 必需操作单项 ≤ 100ms 且单次操作内存上升 ≤ 150MB，数据无损"
        } else if !checksum_ok {
            "✗ FAIL —— 数据一致性校验未通过"
        } else if !mem_ok {
            "✗ FAIL —— 存在必需操作内存上升超阈值"
        } else {
            "✗ FAIL —— 存在必需操作耗时超阈值"
        }
    );
    println!("=== 完成 ===");
}
