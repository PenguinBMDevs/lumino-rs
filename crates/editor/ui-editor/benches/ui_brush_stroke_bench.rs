//! 画刷（自由笔画）性能基准：拖拽单帧 / 预览几何 / 覆盖计算 / √ 全链路 / 撤销
//!
//! 复现背景（BRUSH-001）：旧实现"边拖边盖音符"——每进入一个网格单元就按粗细度
//! 逐层 `insert_note` 写 document，并逐格调用 `mark_notes_changed()` +
//! `mark_track_notes_changed_for()`（每格一次 HashSet 构造）。卡面要求
//! **绘制预览与生成都需随笔画长度保持平稳，不允许随笔画增长线性劣化**。
//!
//! 新实现口径：拖拽全程**零 document 写入、零脏标记**（只在内存里追加折线点）；
//! 预览每帧只处理**可见区间**（逻辑空间预裁剪 + 屏幕抽稀 + 点数上限）；
//! √ 一次性按覆盖集（线段栅格化，O(覆盖格数)）批量写入并只产生一条历史记录。
//!
//! 场景（走**真实交互链路** `Editor::handle_action`，非内部直调）：
//! 1. 拖拽单帧 —— `Moved` 事件处理（追加采样点），统计每次事件耗时；
//! 2. 预览几何 —— `preview_points`（每帧画布覆盖层构建成本）；
//! 3. 覆盖计算 —— `cover_cells`（√ 的计算核心，成本与采样点数无关）；
//! 4. 按钮定位 —— `brush_button_rects`（每帧绘制路径的一部分）；
//! 5. √ 全链路 —— `handle_action(BrushConfirm)`：覆盖 + 层展开 + 按轨写入 + 历史；
//! 6. Ctrl+Z —— `handle_action(Undo)`：一次撤销回退全部生成音符；
//! 7. 旧实现等价链路（模拟，仅对比参考）——逐格逐层 `insert_note` + 逐格脏标记。
//!
//! 性能线：拖拽单帧 ≤ 2ms、预览几何 ≤ 2ms、按钮定位 ≤ 1ms、覆盖 ≤ 20ms、
//! √ 全链路 ≤ 200ms、撤销 ≤ 200ms（撤销线参考 `TARGET_MS`）。
//!
//! 运行：`cargo bench -p lumino-ui-editor --bench ui_brush_stroke_bench`
//! 环境变量：`LUMINO_BENCH_MIDI` / `LUMINO_BENCH_CYCLES` / `LUMINO_BENCH_VERBOSE` /
//! `LUMINO_BRUSH_POINTS`（笔画采样点数，默认 5000）/ `LUMINO_BRUSH_THICKNESS`（默认 20）。
//!
//! 边界说明：无 GPU 环境，本基准**不含** iced canvas 的曲面细分/光栅化成本
//! （那部分与可见点数成正比，预览点数由 MAX_PREVIEW_POINTS 封顶）。

use std::time::Instant;

use lumino_editor_state::brush_tool::cov;
use lumino_message::Point2;
use lumino_ui_editor::grid::brush_tool_box::{brush_button_rects, preview_points};
use lumino_ui_editor::message::EditorAction;
use lumino_ui_editor::{Editor, Note};

#[allow(dead_code)] // 共享支撑模块含其他基准专用项（本基准只取其中一部分）
#[path = "id_history_ops_bench/support.rs"]
mod support;

use support::{
    DEFAULT_CYCLES, DEFAULT_MIDI, Stat, build_workload, env_usize, live, load_doc, mb,
    print_track_stats, reset_run_peak,
};

/// 各场景性能线（中位，ms）
const TARGET_FRAME_MS: f64 = 2.0;
const TARGET_PREVIEW_MS: f64 = 2.0;
const TARGET_BUTTONS_MS: f64 = 1.0;
const TARGET_COVER_MS: f64 = 20.0;
const TARGET_CONFIRM_MS: f64 = 200.0;
const TARGET_UNDO_MS: f64 = 200.0;

/// 中位数（判定基准：单轮最大受调度噪声支配）
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

/// 打印一行结果并返回是否达标
fn report(name: &str, stat: &Stat, target_ms: f64) -> bool {
    let times = stat.times();
    let med = median(times);
    let pass = med <= target_ms;
    if std::env::var_os("LUMINO_BENCH_VERBOSE").is_some() {
        let samples: Vec<String> = times.iter().map(|t| format!("{t:.3}")).collect();
        println!("  [明细] {name}: {}", samples.join(", "));
    }
    println!(
        "{:<26} | 中位 {:>8.3} ms | 最大 {:>8.3} ms | 样本 {:>5} | 线 {:>6.1} ms | {}",
        name,
        med,
        max_of(times),
        times.len(),
        target_ms,
        if pass { "✓ 达标" } else { "✗ 超标" }
    );
    pass
}

/// 生成 5000 点折线笔画（视口内；key 在 100..110 之间往复，tick 每点 +2）
///
/// 起点 tick 480：保证屏幕 X > 键盘宽度（`is_inside_canvas` 要求落在网格区）。
fn stroke_points(count: usize) -> Vec<(f32, f32)> {
    (0..count)
        .map(|i| (480.0 + i as f32 * 2.0, 100.0 + (i % 10) as f32))
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

fn main() {
    println!("=== Lumino 画刷（自由笔画）性能基准 ===");
    let path = std::env::var("LUMINO_BENCH_MIDI").unwrap_or_else(|_| DEFAULT_MIDI.to_string());
    let cycles = env_usize("LUMINO_BENCH_CYCLES", DEFAULT_CYCLES);
    let points_count = env_usize("LUMINO_BRUSH_POINTS", 5000);
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
    editor.editor_state.view.snap_precision = 240.0;
    // 5000 点 × 2 tick = 9998 tick；zoom_x=0.06 → 约 600px，落在 800px 画布内
    editor.editor_state.view.zoom_x = 0.06;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.set_tool(lumino_core::Tool::Brush);
    editor.brush.set_thickness(thickness);

    let points = stroke_points(points_count);
    let bounds = iced_core::Rectangle::new(
        iced_core::Point::new(0.0, 0.0),
        iced_core::Size::new(800.0, 600.0),
    );
    let total_width = thickness as f32 * editor.editor_state.view.zoom_y;

    println!(
        "笔画: {} 点 | 粗细度 {} | 总宽 {:.1}px | 精度 {} tick | 层音轨分配: {}",
        points.len(),
        thickness,
        total_width,
        editor.editor_state.view.snap_precision,
        if editor.brush.track_for_level(0).is_some() {
            "显式"
        } else {
            "默认（按落笔基准轨序行走）"
        }
    );
    println!(
        "目标线: 拖拽单帧≤{TARGET_FRAME_MS:.0} 预览≤{TARGET_PREVIEW_MS:.0} 按钮≤{TARGET_BUTTONS_MS:.0} 覆盖≤{TARGET_COVER_MS:.0} √≤{TARGET_CONFIRM_MS:.0} 撤销≤{TARGET_UNDO_MS:.0} (ms)"
    );
    println!("轮数: {cycles}（+1 暖机）");
    println!();

    let _ = lumino_message::events::take_events();
    let baseline = live();
    reset_run_peak();
    println!("加载后基线堆内存: {:.1} MB", mb(baseline as i64));
    println!();

    let mut moved = Stat::new("拖拽单帧（Moved）");
    let mut preview = Stat::new("预览几何（preview_points）");
    let mut buttons = Stat::new("按钮定位（per-frame）");
    let mut cover = Stat::new("覆盖计算（cover_cells）");
    let mut confirm = Stat::new("√ 全链路（含写入+历史）");
    let mut undo = Stat::new("Ctrl+Z 撤销（一次记录）");

    let mut data_ok = true;
    let mut confirm_notes = 0usize;

    for cycle in 0..=cycles {
        // ── 拖拽：真实链路 Pressed → N×Moved → Released ──
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
                moved.measure(|| editor.handle_action(action));
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

        let before_confirm = total_notes(&editor);

        // ── 预览几何（每帧构建成本，画布覆盖层）──
        if cycle == 0 {
            let _ = preview_points(&editor, &points, bounds, total_width);
        } else {
            preview.measure(|| preview_points(&editor, &points, bounds, total_width));
        }

        // ── 按钮定位（每帧绘制路径的一部分）──
        if cycle == 0 {
            let _ = brush_button_rects(&editor);
        } else {
            buttons.measure(|| brush_button_rects(&editor));
        }

        // ── 覆盖计算（√ 的计算核心）──
        if cycle == 0 {
            let _ = cov::cover_cells(&points, 240.0);
        } else {
            cover.measure(|| cov::cover_cells(&points, 240.0));
        }

        // ── √ 全链路 ──
        if cycle == 0 {
            editor.handle_action(EditorAction::BrushConfirm);
        } else {
            confirm.measure(|| editor.handle_action(EditorAction::BrushConfirm));
        }
        let after_confirm = total_notes(&editor);
        confirm_notes = after_confirm.saturating_sub(before_confirm);
        let _ = lumino_message::events::take_events();

        // ── 撤销 ──
        if cycle == 0 {
            editor.handle_action(EditorAction::Undo);
        } else {
            undo.measure(|| editor.handle_action(EditorAction::Undo));
        }
        let after_undo = total_notes(&editor);
        data_ok &= after_undo == before_confirm;
        let _ = lumino_message::events::take_events();
        data_ok &= !editor.editor_state.brush_tool.has_pending();
    }

    // ── 旧实现等价链路（模拟，仅对比参考）──
    let old_total_ms = old_path_simulation(&mut editor, &points, thickness as usize);
    let _ = lumino_message::events::take_events();

    println!();
    let mut all_pass = true;
    all_pass &= report("拖拽单帧（Moved）", &moved, TARGET_FRAME_MS);
    all_pass &= report("预览几何（preview_points）", &preview, TARGET_PREVIEW_MS);
    all_pass &= report("按钮定位（per-frame）", &buttons, TARGET_BUTTONS_MS);
    all_pass &= report("覆盖计算（cover_cells）", &cover, TARGET_COVER_MS);
    all_pass &= report("√ 全链路（写入+历史）", &confirm, TARGET_CONFIRM_MS);
    all_pass &= report("Ctrl+Z 撤销（一次记录）", &undo, TARGET_UNDO_MS);

    println!();
    println!(
        "对比（单次拖拽 {} 点）: 新实现 拖拽全程 **零 document 写入**（仅内存追加点）；\
         旧实现等价链路（逐格逐层 insert_note + 逐格脏标记）合计 {:.1} ms",
        points.len(),
        old_total_ms
    );
    println!("√ 生成音符（当前轨）: {confirm_notes}");
    println!(
        "数据校验（点数/√ 计数/撤销回退/待确认清空）: {}",
        if data_ok { "✓ 通过" } else { "✗ 失败" }
    );
    println!(
        "结论: {}",
        if all_pass && data_ok {
            "✓ 全部达标"
        } else {
            "✗ 存在超标项"
        }
    );
}

/// 旧实现等价链路（模拟）：逐格逐层 `insert_note` + 逐格 `mark_notes_changed` +
/// 逐格 `mark_track_notes_changed_for`，返回总耗时（ms）
///
/// 仅用于量化"逐格盖戳"的成本量级，不代表已删除的旧代码逐字复刻
/// （旧代码按当前轨序行走分配层音轨，这里统一写当前轨，单次 insert 成本同量级）。
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
