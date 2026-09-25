//! UI 交互路径基准：**框选（连续拖动）+ 复制 + 粘贴（含系统剪贴板）** —— 超大轨道部分选中
//!
//! 与 `id_history_ops_bench`（数据操作层）互补：本基准驱动 `Editor::handle_action`
//! 的真实交互路径，覆盖此前 bench 未覆盖的真实悬崖（APP 端单帧冻结的根因）：
//! 1. **超大轨道无空间索引**（> `SPATIAL_INDEX_MAX_BUILD`=2M）：框选拖动曾退化为
//!    每帧整框重建（19.2M 轨道 ~130ms/帧，且逐索引随机 `get_note_view`）；
//! 2. **部分选中复制**（非「全选快路径」）：曾对全轨逐音符哈希查找，恒等哈希在
//!    索引超表容量时探测退化——实测 19.2M 轨道 / 3M 选中 160s 冻结；
//! 3. **粘贴/大插入的渲染侧全量会话重建**：曾对任何粘贴重建**所有**轨实例
//!    （19.2M 轨粘贴 ~1s：CPU 构建 5200W 实例 + ~900MB GPU 上传），现改为
//!    单轨 `TrackDelta` 主轨段重建（当前轨实例并行分片 + ~400MB）。
//!
//! 工作负载：取音符最多音轨（19.2M），框选其 tick 区间的 `FRACTION`（默认 5%，
//! ≈ 280W 选中）——正是 APP 中的真实形态（轨道远大于选中，走部分选中分支）。
//!
//! 性能线（判定取**中位**；内存=单次操作峰值堆增量）：
//! - 框选单次移动 ≤ 100 ms
//! - 复制（二进制编码）≤ 100 ms
//! - 复制（`handle_action` 全路径，含系统剪贴板写入）≤ 200 ms（含 OS 边界成本）
//! - 释放（`Released`，协作关闭时零广播）≤ 100 ms
//! - 粘贴（`handle_action` 全路径，追加/重叠两种锚点）≤ 250 ms
//! - 主轨段重建（实例构建代理）≤ 200 ms
//! - 单次操作内存上升 ≤ 150 MB（粘贴独立 200 MB；归并路径与实例缓冲仅信息性报道）
//!
//! 运行：
//! ```bash
//! cargo bench -p lumino-ui-editor --bench ui_boxselect_copy_bench
//! LUMINO_UI_BENCH_FRACTION=0.1 LUMINO_UI_BENCH_MOVES=30 LUMINO_UI_BENCH_CYCLES=5 \
//!   cargo bench -p lumino-ui-editor --bench ui_boxselect_copy_bench
//! # 跳过系统剪贴板写入/粘贴（CI/无桌面会话时）：LUMINO_UI_BENCH_CLIPBOARD=0
//! ```
//!
//! 环境变量：`LUMINO_UI_BENCH_MIDI` / `LUMINO_UI_BENCH_FRACTION` / `LUMINO_UI_BENCH_MOVES`
//! / `LUMINO_UI_BENCH_CYCLES` / `LUMINO_UI_BENCH_CLIPBOARD` / `LUMINO_UI_BENCH_VERBOSE`。

use std::env;
use std::time::Instant;

use lumino_ui_editor::Editor;
use lumino_ui_editor::message::EditorAction;

#[path = "ui_boxselect_copy_bench/helpers.rs"]
mod helpers;
#[allow(dead_code)]
#[path = "id_history_ops_bench/support.rs"]
mod support;

use helpers::{
    CycleStats, MEM_TARGET_MB, PASTE_MEM_TARGET_MB, Workload, build_track_instances_proxy,
    env_bool, env_f64, median, report_op, run_cycle,
};
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
/// 粘贴（handle_action 全路径：读剪贴板 + 解码 + 批量插入 + 历史）耗时硬指标（ms）
const PASTE_TARGET_MS: f64 = 250.0;
/// 主轨段重建（实例构建代理）耗时硬指标（ms）
///
/// 渲染侧大粘贴走单轨 `TrackDelta`（构建当前轨实例 + 段替换），
/// 该构建是主轨段重建的主要 CPU 成本；全量会话重建会构建**所有**轨实例。
const REBUILD_TARGET_MS: f64 = 200.0;

fn main() {
    println!("=== Lumino UI 交互路径基准：框选 + 复制 + 粘贴（超大轨道部分选中）===");
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
全路径复制 ≤ {COPY_FULL_TARGET_MS:.0} ms | 释放 ≤ {RELEASE_TARGET_MS:.0} ms | \
粘贴 ≤ {PASTE_TARGET_MS:.0} ms | 主轨段重建 ≤ {REBUILD_TARGET_MS:.0} ms | 单次内存 ≤ {MEM_TARGET_MB:.0} MB"
    );
    println!();

    // ── Editor 装配：真实坐标映射（zoom=1/scroll=0 → x=tick、y=127-key）──
    let mut editor = Editor::new();
    editor.editor_state.data.document = Some(doc);
    editor.editor_state.data.current_track = track;
    // 与生产「未连接协作」路径一致：不构建选择指纹广播载荷
    editor.editor_state.data.set_collab_sync_enabled(false);
    helpers::configure_editor(&mut editor);

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

    // ── 粘贴（数据层）+ 主轨段重建（渲染侧实例构建代理）──
    // 剪贴板内容来自最后一轮 Copy；渲染侧现在以单轨 TrackDelta 重建当前轨段
    // （不再全量会话重建所有轨），实例构建是其主要 CPU 成本 → 作为代理测量。
    let mut paste_stat = Stat::new("粘贴（追加锚点·快路径）");
    let mut paste_merge_stat = Stat::new("粘贴（重叠锚点·归并路径）");
    let mut rebuild_stat = Stat::new("主轨段重建（实例构建代理）");
    if do_clipboard {
        editor.handle_action(EditorAction::Paste); // 暖机
        // 追加锚点：播放位置移到轨道末尾之后 → prepend/append 快路径（无区间归并）
        for _ in 0..2 {
            let end = editor
                .editor_state
                .data
                .current_track_notes()
                .last()
                .map(|n| n.end_tick)
                .unwrap_or(0);
            editor.playback_position = end as f32 + 10.0;
            paste_stat.measure(|| editor.handle_action(EditorAction::Paste));
        }
        // 重叠锚点：固定在 tick 0，与既有粘贴区重叠 → 区间流式归并（最坏路径）
        for _ in 0..2 {
            editor.playback_position = 0.0;
            paste_merge_stat.measure(|| editor.handle_action(EditorAction::Paste));
        }
        let instances = rebuild_stat.measure(|| build_track_instances_proxy(&editor, track));
        all_ok &= instances.iter().any(|p| !p.is_empty());
        drop(instances);
    }

    // ── 汇总 ──
    println!();
    println!("── UI 路径操作耗时 / 内存（{cycles} 轮热态；移动为全部样本合并）──");
    let (move_pass, move_mem) = report_op(
        "框选（单次 Moved）",
        stats.moves.times(),
        MOVE_TARGET_MS,
        stats.moves.max_peak_growth(),
        Some(MEM_TARGET_MB),
    );
    let (released_pass, released_mem) = report_op(
        "释放（Released）",
        stats.released.times(),
        RELEASE_TARGET_MS,
        stats.released.max_peak_growth(),
        Some(MEM_TARGET_MB),
    );
    let (binary_pass, binary_mem) = report_op(
        "复制（二进制编码）",
        stats.binary.times(),
        COPY_TARGET_MS,
        stats.binary.max_peak_growth(),
        Some(MEM_TARGET_MB),
    );
    let (full_pass, full_mem) = if do_clipboard {
        report_op(
            "复制（handle_action 全路径）",
            stats.full_copy.times(),
            COPY_FULL_TARGET_MS,
            stats.full_copy.max_peak_growth(),
            Some(MEM_TARGET_MB),
        )
    } else {
        println!("{:<28} | （剪贴板写入已关闭，跳过）", "复制（全路径）");
        (true, true)
    };
    let (paste_pass, paste_mem) = if do_clipboard {
        report_op(
            "粘贴（追加锚点·快路径）",
            paste_stat.times(),
            PASTE_TARGET_MS,
            paste_stat.max_peak_growth(),
            Some(PASTE_MEM_TARGET_MB),
        )
    } else {
        println!("{:<28} | （剪贴板写入已关闭，跳过）", "粘贴（全路径）");
        (true, true)
    };
    let (paste_merge_pass, _) = if do_clipboard {
        // 重叠锚点走区间流式归并（历史快照 COW 共享 → 归并区间必须复制），
        // 瞬时内存随归并区间长度增长，属固有预算 → 内存仅信息性报道
        report_op(
            "粘贴（重叠锚点·归并路径）",
            paste_merge_stat.times(),
            PASTE_TARGET_MS,
            paste_merge_stat.max_peak_growth(),
            None,
        )
    } else {
        (true, true)
    };
    let (rebuild_pass, _) = if do_clipboard {
        // 实例缓冲（~3000W × 16B ≈ 480MB）是渲染侧固有预算，内存仅信息性报道
        report_op(
            "主轨段重建（实例构建代理）",
            rebuild_stat.times(),
            REBUILD_TARGET_MS,
            rebuild_stat.max_peak_growth(),
            None,
        )
    } else {
        (true, true)
    };

    let mem_pass = move_mem && released_mem && binary_mem && full_mem && paste_mem;

    let peak = run_peak();
    let growth = peak as i64 - baseline as i64;
    println!();
    println!(
        "── 内存总览（信息参考）──\n基线: {:.1} MB | 全程绝对峰值: {:.1} MB | 绝对上升: {:.1} MB（含选中集位图/载荷等跨操作保留）",
        mb(baseline as i64),
        mb(peak as i64),
        mb(growth)
    );

    let pass = all_ok
        && move_pass
        && released_pass
        && binary_pass
        && full_pass
        && paste_pass
        && paste_merge_pass
        && rebuild_pass
        && mem_pass;
    println!();
    println!(
        "结论（超大轨道部分选中 框选+复制+粘贴）: {}",
        if pass {
            "✓ PASS —— 框选移动/释放/复制/粘贴/主轨段重建均在性能线内，数据校验通过"
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
