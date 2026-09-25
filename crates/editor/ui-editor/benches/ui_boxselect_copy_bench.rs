//! UI 交互路径基准：**框选（连续拖动）+ 复制（含系统剪贴板）** —— 超大轨道部分选中
//!
//! 与 `id_history_ops_bench`（数据操作层）互补：本基准驱动 `Editor::handle_action`
//! 的真实交互路径，覆盖此前 bench 未覆盖的两个真实悬崖（APP 端 105s 单帧冻结的根因）：
//! 1. **超大轨道无空间索引**（> `SPATIAL_INDEX_MAX_BUILD`=2M）：框选拖动曾退化为
//!    每帧整框重建（19.2M 轨道 ~130ms/帧，且逐索引随机 `get_note_view`）；
//! 2. **部分选中复制**（非「全选快路径」）：曾对全轨逐音符哈希查找，恒等哈希在
//!    索引超表容量时探测退化——实测 19.2M 轨道 / 3M 选中 160s 冻结。
//!
//! 工作负载：取音符最多音轨（19.2M），框选其 tick 区间的 `FRACTION`（默认 5%，
//! ≈ 280W 选中）——正是 APP 中的真实形态（轨道远大于选中，走部分选中分支）。
//!
//! 性能线（判定取**中位**；内存=单次操作峰值堆增量）：
//! - 框选单次移动 ≤ 100 ms
//! - 复制（二进制编码）≤ 100 ms
//! - 复制（`handle_action` 全路径，含系统剪贴板写入）≤ 200 ms（含 OS 边界成本）
//! - 释放（`Released`，协作关闭时零广播）≤ 100 ms
//! - 单次操作内存上升 ≤ 150 MB
//!
//! 运行：
//! ```bash
//! cargo bench -p lumino-ui-editor --bench ui_boxselect_copy_bench
//! LUMINO_UI_BENCH_FRACTION=0.1 LUMINO_UI_BENCH_MOVES=30 LUMINO_UI_BENCH_CYCLES=5 \
//!   cargo bench -p lumino-ui-editor --bench ui_boxselect_copy_bench
//! # 跳过系统剪贴板写入（CI/无桌面会话时）：LUMINO_UI_BENCH_CLIPBOARD=0
//! ```
//!
//! 环境变量：`LUMINO_UI_BENCH_MIDI` / `LUMINO_UI_BENCH_FRACTION` / `LUMINO_UI_BENCH_MOVES`
//! / `LUMINO_UI_BENCH_CYCLES` / `LUMINO_UI_BENCH_CLIPBOARD` / `LUMINO_UI_BENCH_VERBOSE`。

use std::env;
use std::time::Instant;

use lumino_ui_editor::message::{EditorAction, Point2, Tool};
use lumino_ui_editor::{EditState, Editor};

#[allow(dead_code)]
#[path = "id_history_ops_bench/support.rs"]
mod support;

use support::{Stat, env_usize, live, mb, reset_run_peak, run_peak};

const DEFAULT_MIDI: &str = r"D:\BM-DATA\MIDI File\Toilet Story 6 F2.mid";
/// 框选默认比例（占全轨 tick 区间；5% ≈ 280W 选中）
const DEFAULT_FRACTION: f64 = 0.05;
/// 框选默认移动事件数（真实拖动一帧一次）
const DEFAULT_MOVES: usize = 30;
/// 默认热态轮数（另加 1 轮暖机）
const DEFAULT_CYCLES: usize = 5;
/// 框选单次移动耗时硬指标（ms）
const MOVE_TARGET_MS: f64 = 100.0;
/// 复制（二进制编码）耗时硬指标（ms）
const COPY_TARGET_MS: f64 = 100.0;
/// 复制（handle_action 全路径，含 OS 剪贴板写入）耗时硬指标（ms）
const COPY_FULL_TARGET_MS: f64 = 200.0;
/// 释放耗时硬指标（ms）
const RELEASE_TARGET_MS: f64 = 100.0;
/// 单次操作内存上升硬指标（MB）
const MEM_TARGET_MB: f64 = 150.0;

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key).ok().map(|v| v != "0").unwrap_or(default)
}

/// 单轮测量统计
struct CycleStats {
    moves: Stat,
    released: Stat,
    binary: Stat,
    full_copy: Stat,
    selected: usize,
    payload_len: usize,
}

impl CycleStats {
    fn new() -> Self {
        Self {
            moves: Stat::new("框选（单次 Moved）"),
            released: Stat::new("释放（Released）"),
            binary: Stat::new("复制（二进制编码）"),
            full_copy: Stat::new("复制（handle_action 全路径）"),
            selected: 0,
            payload_len: 0,
        }
    }
}

/// 工作负载与运行参数（打包传递，避免 `run_cycle` 参数过多）
struct Workload {
    x0: f32,
    sel_end: f32,
    moves: usize,
    track: usize,
    division: u16,
    do_clipboard: bool,
}

/// 执行一轮：框选（Pressed → N×Moved → Released）→ 二进制编码 → 全路径复制 → 复位。
///
/// 返回数据校验是否通过（选中数与载荷均非空）。
fn run_cycle(editor: &mut Editor, stats: &mut CycleStats, w: &Workload) -> bool {
    // 起点在首个音符之前（无音符处）→ 必然进入框选而非音符编辑
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(w.x0, 127.0),
        shift: false,
        ctrl: false,
    });
    if !matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Selecting { .. }
    ) {
        eprintln!("✗ 未进入框选状态（按下命中音符？）");
        return false;
    }

    for i in 1..=w.moves {
        let f = i as f32 / w.moves as f32;
        let x = w.x0 + (w.sel_end - w.x0) * f;
        let y = 127.0 * (1.0 - f);
        stats
            .moves
            .measure(|| editor.handle_action(EditorAction::Moved(Point2::new(x, y))));
    }

    stats
        .released
        .measure(|| editor.handle_action(EditorAction::Released));

    let payload = stats
        .binary
        .measure(|| editor.build_clipboard_binary(w.track, w.division));
    let payload_len = payload.as_ref().map(|b| b.len()).unwrap_or(0);
    drop(payload);

    if w.do_clipboard {
        stats
            .full_copy
            .measure(|| editor.handle_action(EditorAction::Copy));
    }

    stats.selected = editor.selected_notes_count();
    stats.payload_len = payload_len;

    // 复位：清空选中（下一轮从零开始框选）
    editor.clear_selection();
    editor.editor_state.interaction.edit_state = EditState::Idle;

    stats.selected > 0 && payload_len > 0
}

fn main() {
    println!("=== Lumino UI 交互路径基准：框选 + 复制（超大轨道部分选中）===");
    let path = env::var("LUMINO_UI_BENCH_MIDI").unwrap_or_else(|_| DEFAULT_MIDI.to_string());
    let fraction = env_f64("LUMINO_UI_BENCH_FRACTION", DEFAULT_FRACTION).clamp(0.001, 1.0);
    let moves = env_usize("LUMINO_UI_BENCH_MOVES", DEFAULT_MOVES).max(1);
    let cycles = env_usize("LUMINO_UI_BENCH_CYCLES", DEFAULT_CYCLES);
    let do_clipboard = env_bool("LUMINO_UI_BENCH_CLIPBOARD", true);

    let t_load = Instant::now();
    let (doc, src) = support::load_doc(&path);
    let division = doc.division();
    let track = (0..doc.track_count())
        .max_by_key(|&t| doc.track_notes(t).len())
        .expect("无音轨");
    let track_len = doc.track_notes(track).len();
    println!(
        "数据源: {src} | 最大轨 {track} / {track_len} 音符 | division {division} | 加载 {:.1}s",
        t_load.elapsed().as_secs_f64()
    );
    println!(
        "工作负载: 框选 tick 区间 {:.0}% | 移动 {moves} 次/轮 | 轮数 {cycles}(+1 暖机) | 剪贴板写入 {}",
        fraction * 100.0,
        if do_clipboard {
            "开"
        } else {
            "关（仅编码）"
        }
    );
    println!(
        "目标线: 移动 ≤ {MOVE_TARGET_MS:.0} ms | 二进制复制 ≤ {COPY_TARGET_MS:.0} ms | \
全路径复制 ≤ {COPY_FULL_TARGET_MS:.0} ms | 释放 ≤ {RELEASE_TARGET_MS:.0} ms | 单次内存 ≤ {MEM_TARGET_MB:.0} MB"
    );
    println!();

    // ── Editor 装配：真实坐标映射（zoom=1/scroll=0 → x=tick、y=127-key）──
    let mut editor = Editor::new();
    editor.editor_state.data.document = Some(doc);
    editor.editor_state.data.current_track = track;
    // 与生产「未连接协作」路径一致：不构建选择指纹广播载荷
    editor.editor_state.data.set_collab_sync_enabled(false);
    editor.editor_state.tool = Tool::Pointer;
    {
        let v = &mut editor.editor_state.view;
        v.visible_key_count = 128;
        v.zoom_x = 1.0;
        v.scroll_x = 0.0;
        v.keyboard_width = 0.0;
        v.zoom_y = 1.0;
        v.scroll_y = 0.0;
        v.ruler_height = 0.0;
    }
    editor.editor_state.canvas.size_x = 4_000_000.0;
    editor.editor_state.canvas.size_y = 200.0;

    let first_tick = editor
        .editor_state
        .data
        .current_track_notes()
        .first()
        .map(|n| n.start_tick)
        .unwrap_or(0);
    let last_tick = editor
        .editor_state
        .data
        .current_track_notes()
        .last()
        .map(|n| n.start_tick)
        .unwrap_or(1);
    let span = last_tick.saturating_sub(first_tick) as f64;
    let sel_end = (first_tick as f64 + span * fraction) as f32;
    let x0 = (first_tick as f32 - 10.0).max(0.0);

    let baseline = live();
    reset_run_peak();

    let workload = Workload {
        x0,
        sel_end,
        moves,
        track,
        division,
        do_clipboard,
    };

    // ── 冷启动暖机一轮（仅参考，不计入判定）──
    let mut warmup = CycleStats::new();
    let warmup_ok = run_cycle(&mut editor, &mut warmup, &workload);
    println!(
        "冷启动首轮（仅参考）: 移动中位 {:.1} ms | 释放 {:.1} ms | 二进制 {:.1} ms | 全路径 {} | 选中 {} | 校验 {}",
        median(warmup.moves.times()),
        median(warmup.released.times()),
        median(warmup.binary.times()),
        warmup
            .full_copy
            .times()
            .first()
            .map(|v| format!("{v:.1} ms"))
            .unwrap_or_else(|| "—".into()),
        warmup.selected,
        if warmup_ok { "✓" } else { "✗" }
    );
    println!();

    // ── 热态连续 N 轮（判定依据）──
    let mut stats = CycleStats::new();
    let mut all_ok = warmup_ok;
    for c in 0..cycles {
        let moves_before = stats.moves.times().len();
        let ok = run_cycle(&mut editor, &mut stats, &workload);
        all_ok &= ok;
        let cycle_moves = &stats.moves.times()[moves_before..];
        println!(
            "第 {}/{} 轮: 移动中位 {:.1} ms（最大 {:.1}）| 释放 {:.1} ms | 二进制 {:.1} ms | 全路径 {} | 选中 {} | 载荷 {} B | 校验 {}",
            c + 1,
            cycles,
            median(cycle_moves),
            cycle_moves.iter().copied().fold(0.0f64, f64::max),
            stats.released.times().last().copied().unwrap_or_default(),
            stats.binary.times().last().copied().unwrap_or_default(),
            stats
                .full_copy
                .times()
                .last()
                .map(|v| format!("{v:.1} ms"))
                .unwrap_or_else(|| "—".into()),
            stats.selected,
            stats.payload_len,
            if ok { "✓" } else { "✗" }
        );
    }

    // ── 汇总 ──
    println!();
    println!("── UI 路径操作耗时 / 内存（{cycles} 轮热态；移动为全部样本合并）──");
    let move_pass = report_op("框选（单次 Moved）", stats.moves.times(), MOVE_TARGET_MS);
    let move_mem = mb(stats.moves.max_peak_growth()) <= MEM_TARGET_MB;
    let released_pass = report_op(
        "释放（Released）",
        stats.released.times(),
        RELEASE_TARGET_MS,
    );
    let binary_pass = report_op("复制（二进制编码）", stats.binary.times(), COPY_TARGET_MS);
    let full_pass = if do_clipboard {
        report_op(
            "复制（handle_action 全路径）",
            stats.full_copy.times(),
            COPY_FULL_TARGET_MS,
        )
    } else {
        println!("{:<28} | （剪贴板写入已关闭，跳过）", "复制（全路径）");
        true
    };

    let mem_pass = move_mem
        && mb(stats.released.max_peak_growth()) <= MEM_TARGET_MB
        && mb(stats.binary.max_peak_growth()) <= MEM_TARGET_MB
        && (!do_clipboard || mb(stats.full_copy.max_peak_growth()) <= MEM_TARGET_MB);

    let peak = run_peak();
    let growth = peak as i64 - baseline as i64;
    println!();
    println!(
        "── 内存总览（信息参考）──\n基线: {:.1} MB | 全程绝对峰值: {:.1} MB | 绝对上升: {:.1} MB（含选中集位图/载荷等跨操作保留）",
        mb(baseline as i64),
        mb(peak as i64),
        mb(growth)
    );

    let pass = all_ok && move_pass && released_pass && binary_pass && full_pass && mem_pass;
    println!();
    println!(
        "结论（超大轨道部分选中 框选+复制）: {}",
        if pass {
            "✓ PASS —— 框选移动/释放/复制均在性能线内，数据校验通过"
        } else if !all_ok {
            "✗ FAIL —— 数据校验失败"
        } else if !mem_pass {
            "✗ FAIL —— 存在单次操作内存上升超阈值"
        } else {
            "✗ FAIL —— 存在操作耗时超阈值"
        }
    );
    println!("=== 完成 ===");
}

/// 输出单操作统计（中位/最小/最大），返回耗时是否达标。
fn report_op(name: &str, times: &[f64], target_ms: f64) -> bool {
    if times.is_empty() {
        println!("{name:<28} | （无样本）");
        return true;
    }
    let median = median(times);
    let max = times.iter().copied().fold(0.0f64, f64::max);
    let min = times.iter().copied().fold(f64::INFINITY, f64::min);
    let pass = median <= target_ms;
    if env::var_os("LUMINO_UI_BENCH_VERBOSE").is_some() {
        let samples: Vec<String> = times.iter().map(|t| format!("{t:.1}")).collect();
        println!("  [明细] {name}: {} ms", samples.join(", "));
    }
    println!(
        "{name:<28} | 中位 {median:>8.2} ms | 最小 {min:>8.2} ms | 最大 {max:>8.2} ms | {}",
        if pass {
            "✓ 达标"
        } else {
            "✗ 耗时超标"
        }
    );
    pass
}

fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut s = xs.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    s[s.len() / 2]
}
