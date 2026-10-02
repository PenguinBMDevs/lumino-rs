//! 画刷（自由笔画）性能基准：拖拽单帧 / 预览构建 / 覆盖计算 / √ 全链路 / 撤销
//!
//! 复现背景（BRUSH-001）：
//! - 旧实现"边拖边盖音符"——每进入一个网格单元就按粗细度逐层 `insert_note`
//!   写 document，并逐格 `mark_notes_changed()` + `mark_track_notes_changed_for()`；
//! - 新实现：拖拽全程**零 document 写入**（只追加折线点）；预览按**覆盖格方块**
//!   绘制（与 √ 生成同源，所见即生成）；√ 批量归并写入 + 一条历史记录；
//!   撤销改为**按轨区间合并删除**（旧逐 op `remove_note` 每次 `rebuild_index`）。
//!
//! 两档负载（与 `ui_delete_bench` 的"计入判定 / 仅参考"同范式）：
//! - **A 真实长笔画**（计入判定）：1000 点 × 1 格/点 × 粗细度 20 → 2 万音符，
//!   音高平滑走向（真实手绘形态，行段合并有效）；
//! - **B 病态锯齿**（仅信息参考）：5000 点 × 1 格/点 × 音高每点跳变 → 34 万音符，
//!   行段几乎无法合并（覆盖格数 × 层数的极端上界）。
//!
//! 场景（均走真实交互链路 `Editor::handle_action`，非内部直调）：
//! 1. 拖拽单帧（`Moved` 处理）；2. 预览构建（行段 + 方块）；3. 按钮定位；
//! 4. 覆盖计算（`cover_cells`）；5. √ 全链路（覆盖 + 层展开 + 写入 + 历史）；
//! 6. Ctrl+Z 撤销（一次记录回退全部音符）；
//! 7. 旧实现等价链路（模拟，仅前 100 点，折算每音符成本）；
//! 8. **C/C2 增长曲线**（见 [`growth`]）：每帧预览成本 × 笔画长度 —— 长笔画掉帧的
//!    量化证据与回归门控（A/B 只有两个点，连不成结论）。
//!
//! 运行：`cargo bench -p lumino-ui-editor --bench ui_brush_stroke_bench`
//! 环境变量：`LUMINO_BENCH_MIDI` / `LUMINO_BENCH_CYCLES` / `LUMINO_BENCH_VERBOSE`。
//!
//! 边界：无 GPU 环境，**不含** iced canvas 曲面细分/光栅化成本（那部分与可见方块数
//! 成正比，绘制前已按格 + 屏幕矩形双重剔除）。

use std::time::Instant;

use iced_core::{Point, Rectangle, Size};
use lumino_editor_state::brush_tool::cov;
use lumino_message::Point2;
use lumino_ui_editor::grid::brush_tool_box::{
    brush_button_rects, brush_cell_rect, brush_run_screen_rect, brush_visible_window,
};
use lumino_ui_editor::message::EditorAction;
use lumino_ui_editor::{Editor, Note};

#[path = "ui_brush_stroke_bench/growth.rs"]
mod growth;

/// 增长曲线的笔画长度序列（点数）
const GROWTH_LENS: [usize; 5] = [1000, 2000, 5000, 10000, 20000];
/// 「边画边测」的里程碑（点数）
const IN_STROKE_MARKS: [usize; 4] = [1000, 5000, 10000, 20000];

#[allow(dead_code)] // 共享支撑模块含其他基准专用项（本基准只取其中一部分）
#[path = "id_history_ops_bench/support.rs"]
mod support;

use support::{
    DEFAULT_CYCLES, DEFAULT_MIDI, Stat, build_workload, env_usize, live, load_doc, mb,
    print_track_stats, reset_run_peak,
};

/// 性能线（中位，ms）
const TARGET_FRAME_MS: f64 = 2.0;
const TARGET_PREVIEW_MS: f64 = 2.0;
const TARGET_BUTTONS_MS: f64 = 1.0;
const TARGET_COVER_MS: f64 = 20.0;
const TARGET_CONFIRM_MS: f64 = 200.0;
const TARGET_UNDO_MS: f64 = 200.0;
/// 长笔画窗口化预览线（ms）= 帧预算的 1/4
///
/// 为什么不沿用 A 档的 2ms：C 档是**故意病态**的锯齿负载（粗细度 20 下视口内就有
/// 2.5 万个互不相连的方块）——这些方块是"必须画"的可见工作量，不是被浪费的算力。
/// 该门控要抓的是"成本随笔画长度增长"：绝对值不许吃掉 1/4 帧预算，
/// 且增长倍数必须接近 1×（线性增长 = O(笔画长度) 回归）。
const TARGET_LONG_STROKE_MS: f64 = 4.0;

/// 负载规格
struct Scenario {
    name: &'static str,
    /// 采样点数
    points: usize,
    /// 每点 tick 步进（240 = 一个吸附格 → 每点覆盖新格）
    step: f32,
    /// 音高走向：true = 平滑爬升（真实手绘），false = 每点跳变（病态锯齿）
    smooth_pitch: bool,
    /// 是否计入 PASS/FAIL（false = 仅信息参考）
    gated: bool,
}

fn median(times: &[f64]) -> f64 {
    if times.is_empty() {
        return 0.0;
    }
    let mut sorted = times.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted[sorted.len() / 2]
}

fn max_of(times: &[f64]) -> f64 {
    times.iter().copied().fold(0.0f64, f64::max)
}

/// 打印一行结果并返回是否达标（`gated=false` 时不参与总判定）
fn report(name: &str, stat: &Stat, target_ms: f64, gated: bool) -> bool {
    let times = stat.times();
    let med = median(times);
    let pass = !gated || med <= target_ms;
    if std::env::var_os("LUMINO_BENCH_VERBOSE").is_some() {
        let samples: Vec<String> = times.iter().map(|t| format!("{t:.3}")).collect();
        println!("  [明细] {name}: {}", samples.join(", "));
    }
    println!(
        "{:<26} | 中位 {:>9.3} ms | 最大 {:>9.3} ms | 样本 {:>5} | 线 {:>6.1} ms | {}",
        name,
        med,
        max_of(times),
        times.len(),
        target_ms,
        if !gated {
            "（仅参考）"
        } else if pass {
            "✓ 达标"
        } else {
            "✗ 超标"
        }
    );
    pass
}

/// 生成笔画折线（视口内；起点 tick 480 保证落在网格区）
fn stroke_points(spec: &Scenario) -> Vec<(f32, f32)> {
    (0..spec.points)
        .map(|i| {
            let tick = 480.0 + i as f32 * spec.step;
            let key = if spec.smooth_pitch {
                100.0 + (i as f32 / 120.0)
            } else {
                100.0 + (i % 10) as f32
            };
            (tick, key)
        })
        .collect()
}

/// 文档音符总数（全轨）
fn total_notes(editor: &Editor) -> usize {
    let Some(doc) = editor.editor_state.data.document.as_ref() else {
        return 0;
    };
    (0..doc.track_count())
        .map(|t| doc.track_notes(t).len())
        .sum()
}

/// 单场景统计
struct Stats {
    moved: Stat,
    preview: Stat,
    buttons: Stat,
    cover: Stat,
    confirm: Stat,
    undo: Stat,
}

impl Stats {
    fn new() -> Self {
        Self {
            moved: Stat::new("拖拽单帧（Moved）"),
            preview: Stat::new("预览构建（行段+方块）"),
            buttons: Stat::new("按钮定位（per-frame）"),
            cover: Stat::new("覆盖计算（cover_cells）"),
            confirm: Stat::new("√ 全链路（写入+历史）"),
            undo: Stat::new("Ctrl+Z 撤销（一次记录）"),
        }
    }
}

/// 每帧预览构建：行段（与 √ 生成同源）+ 每段方块矩形；返回行段数
///
/// **生产绘制路径（窗口化）**：只对可见窗口内的格做覆盖/行段计算 ——
/// 与 `grid::brush_tool_box::draw` 完全同一条管线（同一组函数），
/// 因此本指标就是"用户拖长笔画时每帧要付的账"。
fn preview_runs_and_rects(editor: &Editor) -> usize {
    let bounds = canvas_bounds(editor);
    let window = brush_visible_window(editor, bounds);
    let runs = editor.brush_preview_runs_in_window(window);
    for run in &runs {
        let _ = brush_run_screen_rect(editor, &window, bounds, *run);
    }
    runs.len()
}

/// 旧口径（对照，仅用于 A/B 量化）：全量栅格化 + 全量行段 + 逐段方块矩形
///
/// 这是 §17 修复前的生产路径（对整笔做覆盖计算后再逐段剔除），保留它做
/// **同机同轮 A/B**——修复效果不能靠"感觉快了"，要有同一张表的两列数字。
fn preview_full_path(editor: &Editor) -> usize {
    let runs = editor.brush_preview_runs();
    for (_, key, t_start, t_end) in &runs {
        let _ = brush_cell_rect(editor, *t_start, *t_end + 1, *key);
    }
    runs.len()
}

/// 画布 bounds（画布局部坐标；尺寸取自编辑器）
pub fn canvas_bounds(editor: &Editor) -> Rectangle {
    Rectangle::new(
        Point::new(0.0, 0.0),
        Size::new(
            editor.editor_state.canvas.size_x,
            editor.editor_state.canvas.size_y,
        ),
    )
}

/// 跑一个场景，返回（统计, 数据校验, 生成音符数, 行段数）
fn run_scenario(
    editor: &mut Editor,
    spec: &Scenario,
    cycles: usize,
) -> (Stats, bool, usize, usize) {
    let points = stroke_points(spec);
    let span_ticks = points.len() as f32 * spec.step;
    // 1px / 格：屏幕坐标与格一一对应，避免极端缩放下采样往返丢格
    editor.editor_state.view.zoom_x = 1.0 / spec.step.max(1.0);
    editor.editor_state.view.snap_precision = 240.0;
    let _ = span_ticks;

    let mut stats = Stats::new();
    let mut data_ok = true;
    let mut confirm_notes = 0usize;
    let mut runs_count = 0usize;

    for cycle in 0..=cycles {
        let start_screen = editor.line_pos_screen_pos(points[0]);
        editor.handle_action(EditorAction::Pressed {
            pos: Point2::new(start_screen.x, start_screen.y),
            shift: false,
            ctrl: false,
        });
        for point in points.iter().skip(1) {
            let screen = editor.line_pos_screen_pos(*point);
            let action = EditorAction::Moved(Point2::new(screen.x, screen.y));
            if cycle == 0 {
                editor.handle_action(action);
            } else {
                stats.moved.measure(|| editor.handle_action(action));
            }
        }
        editor.handle_action(EditorAction::Released);

        let stroke_points = editor
            .editor_state
            .brush_tool
            .strokes
            .first()
            .map(|s| s.points.len())
            .unwrap_or(0);
        data_ok &= stroke_points == points.len();

        let before_confirm = total_notes(editor);

        if cycle == 0 {
            runs_count = preview_runs_and_rects(editor);
        } else {
            stats
                .preview
                .measure(|| runs_count = preview_runs_and_rects(editor));
        }

        if cycle == 0 {
            let _ = brush_button_rects(editor);
        } else {
            stats.buttons.measure(|| brush_button_rects(editor));
        }

        if cycle == 0 {
            let _ = cov::cover_cells(&points, 240.0);
        } else {
            stats.cover.measure(|| cov::cover_cells(&points, 240.0));
        }

        if cycle == 0 {
            editor.handle_action(EditorAction::BrushConfirm);
        } else {
            stats
                .confirm
                .measure(|| editor.handle_action(EditorAction::BrushConfirm));
        }
        confirm_notes = total_notes(editor).saturating_sub(before_confirm);
        let _ = lumino_message::events::take_events();

        if cycle == 0 {
            editor.handle_action(EditorAction::Undo);
        } else {
            stats
                .undo
                .measure(|| editor.handle_action(EditorAction::Undo));
        }
        let after_undo = total_notes(editor);
        data_ok &= after_undo == before_confirm;
        let _ = lumino_message::events::take_events();
        data_ok &= !editor.editor_state.brush_tool.has_pending();
    }

    (stats, data_ok, confirm_notes, runs_count)
}

fn main() {
    println!("=== Lumino 画刷（自由笔画）性能基准 ===");
    let path = std::env::var("LUMINO_BENCH_MIDI").unwrap_or_else(|_| DEFAULT_MIDI.to_string());
    let cycles = env_usize("LUMINO_BENCH_CYCLES", DEFAULT_CYCLES);
    let thickness = env_usize("LUMINO_BRUSH_THICKNESS", 20) as u8;

    let t_load = Instant::now();
    let (doc, src) = load_doc(&path);
    print_track_stats(&doc);
    let (doc, track, total) = build_workload(doc, usize::MAX, true);
    println!(
        "数据源: {src} | 轨 {track} / {total} 音符 | 加载 {:.1}s",
        t_load.elapsed().as_secs_f64()
    );

    let mut editor = Editor::new();
    editor.editor_state.data.document = Some(doc);
    editor.editor_state.data.current_track = track.max(1);
    editor.editor_state.data.set_collab_sync_enabled(false);
    editor.editor_state.view.visible_key_count = 128;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.set_tool(lumino_core::Tool::Brush);
    editor.brush.set_thickness(thickness);
    let _ = lumino_message::events::take_events();
    let baseline = live();
    reset_run_peak();
    println!(
        "基线堆内存: {:.1} MB | 粗细度 {thickness}",
        mb(baseline as i64)
    );
    println!(
        "目标线: 拖拽单帧≤{TARGET_FRAME_MS:.0} 预览≤{TARGET_PREVIEW_MS:.0} 按钮≤{TARGET_BUTTONS_MS:.0} 覆盖≤{TARGET_COVER_MS:.0} √≤{TARGET_CONFIRM_MS:.0} 撤销≤{TARGET_UNDO_MS:.0} (ms)"
    );

    let scenarios = [
        Scenario {
            name: "A 真实长笔画（1000 点 / 2 万音符）",
            points: 1000,
            step: 240.0,
            smooth_pitch: true,
            gated: true,
        },
        Scenario {
            name: "B 病态锯齿（5000 点 / 34 万音符）",
            points: 5000,
            step: 240.0,
            smooth_pitch: false,
            gated: false,
        },
    ];

    let mut all_pass = true;
    let mut data_ok = true;
    for spec in &scenarios {
        println!();
        println!("── {}（轮数 {cycles}+1 暖机） ──", spec.name);
        let (stats, ok, notes, runs) = run_scenario(&mut editor, spec, cycles);
        data_ok &= ok;
        all_pass &= report(
            "拖拽单帧（Moved）",
            &stats.moved,
            TARGET_FRAME_MS,
            spec.gated,
        );
        all_pass &= report(
            "预览构建（行段+方块）",
            &stats.preview,
            TARGET_PREVIEW_MS,
            spec.gated,
        );
        all_pass &= report(
            "按钮定位（per-frame）",
            &stats.buttons,
            TARGET_BUTTONS_MS,
            spec.gated,
        );
        all_pass &= report(
            "覆盖计算（cover_cells）",
            &stats.cover,
            TARGET_COVER_MS,
            spec.gated,
        );
        all_pass &= report(
            "√ 全链路（写入+历史）",
            &stats.confirm,
            TARGET_CONFIRM_MS,
            spec.gated,
        );
        all_pass &= report(
            "Ctrl+Z 撤销（一次记录）",
            &stats.undo,
            TARGET_UNDO_MS,
            spec.gated,
        );
        println!(
            "  生成音符 {notes} | 预览行段 {runs} | 每音符撤销成本 {:.2} µs | 数据校验 {}",
            if notes > 0 {
                median(stats.undo.times()) * 1000.0 / notes as f64
            } else {
                0.0
            },
            if ok { "✓" } else { "✗" }
        );
    }

    // ── C 增长曲线：每帧成本 vs 笔画长度（长笔画掉帧根因量化） ──
    println!();
    let growth_cycles = cycles.max(4);
    let rows = growth::run_growth_curve(&mut editor, &GROWTH_LENS, growth_cycles);
    all_pass &= growth::report_growth(&rows, thickness, TARGET_LONG_STROKE_MS);
    let curve = growth::run_growth_in_stroke(&mut editor, &IN_STROKE_MARKS, growth_cycles);
    all_pass &= growth::report_in_stroke(&curve, TARGET_LONG_STROKE_MS);
    let _ = lumino_message::events::take_events();

    // ── 旧实现等价链路（模拟，仅前 200 点，折算每音符成本） ──
    println!();
    let probe = stroke_points(&Scenario {
        name: "probe",
        points: 200,
        step: 240.0,
        smooth_pitch: true,
        gated: false,
    });
    let old_ms = old_path_simulation(&mut editor, &probe, thickness as usize);
    let old_notes = probe.len() * thickness as usize;
    println!(
        "对比（旧实现等价链路，前 {} 点 ≈ {} 音符）: {:.1} ms → 每音符 {:.2} µs；\
         新实现同规模 √ 见上表（批量归并）",
        probe.len(),
        old_notes,
        old_ms,
        old_ms * 1000.0 / old_notes as f64
    );
    let _ = lumino_message::events::take_events();

    println!();
    println!(
        "结论: {}",
        if all_pass && data_ok {
            "✓ 计入判定的场景全部达标"
        } else {
            "✗ 存在超标/数据校验失败项"
        }
    );
}

/// 旧实现等价链路（模拟）：逐格逐层 `insert_note` + 逐格 `mark_notes_changed` +
/// 逐格 `mark_track_notes_changed_for`，返回总耗时（ms）
///
/// 仅用于量化"逐格盖戳"的成本量级（单次 `insert_note` 成本同量级），
/// 不代表已删除的旧代码逐字复刻。
fn old_path_simulation(editor: &mut Editor, points: &[(f32, f32)], thickness: usize) -> f64 {
    let track = editor.editor_state.data.current_track;
    let snap = editor.editor_state.view.snap_precision.max(1.0);
    let t0 = Instant::now();
    let mut last_cell: Option<(i64, u16)> = None;
    for &(tick, key) in points {
        let cell = ((tick / snap).floor() as i64, key.round() as u16);
        if last_cell == Some(cell) {
            continue;
        }
        last_cell = Some(cell);
        let mut affected = std::collections::HashSet::new();
        for level in 0..thickness {
            let k = (key as u16).saturating_add(level as u16);
            if k > u8::MAX as u16 {
                break;
            }
            let note = Note::new(tick, k, snap);
            editor.editor_state.data.insert_note(track, note);
            affected.insert(track);
        }
        editor.mark_notes_changed();
        editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
    }
    t0.elapsed().as_secs_f64() * 1000.0
}
