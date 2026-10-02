//! 增长曲线：把「每帧预览成本」画成「笔画长度」的函数——掉帧根因的量化与回归门控
//!
//! 为什么要单独一个模块：A/B 两档只给了两个点（2 万 / 34 万音符），
//! **两点连不成结论**。掉帧的本质是"成本随笔画长度线性增长直到撞破帧预算"，
//! 必须画出曲线才能：
//! 1. 证明根因是 O(笔画长度) 而不是别的（噪声/GC/视口）；
//! 2. 证明"视口外计算"占比（全量行段数 vs 窗口化行段数）；
//! 3. 修完用同一条曲线做 A/B（曲线压平 = 修好了，不是"感觉快了"）。
//!
//! 两个视角：
//! - [`run_growth_curve`]：不同长度的独立笔画，各测一次稳态每帧成本（成本函数），
//!   同一行**同时**报旧口径（全量栅格化）与新口径（窗口化）→ 同机同轮 A/B；
//! - [`run_growth_in_stroke`]：**同一笔边画边测**（真实掉帧曲线：一笔越画越卡）。
//!
//! 判定口径：窗口化路径每行每帧成本 ≤ [`super::TARGET_PREVIEW_MS`]（2ms 预览线），
//! 并在超 [`FRAME_BUDGET_MS`] 时标注「掉帧」——修复前预期直接 ✗。

use lumino_message::Point2;
use lumino_ui_editor::Editor;
use lumino_ui_editor::message::EditorAction;

use super::{
    Scenario, Stat, mb, median, preview_canvas_geometry, preview_full_path, preview_runs_and_rects,
    stroke_points,
};

/// 60fps 帧预算（ms）——超过即肉眼可见掉帧
pub const FRAME_BUDGET_MS: f64 = 16.67;

/// 一个笔画长度下的观测
pub struct GrowthRow {
    /// 采样点数
    pub points: usize,
    /// 全量覆盖格数（去重后，单层）
    pub cells: usize,
    /// 旧口径行段数（全量栅格化）
    pub runs_full: usize,
    /// 生产路径实例数（§18 wgpu：= 可见行段数）
    pub runs_window: usize,
    /// 旧口径每帧耗时中位（ms，§17 之前的全量画布几何）
    pub full_ms: f64,
    /// §17 画布几何（窗口化行段 + 矩形）耗时中位（ms）
    pub canvas_ms: f64,
    /// §18 生产路径（wgpu 预览实例构建）耗时中位（ms）——判定基准
    pub window_ms: f64,
    /// 旧口径单次构建累计新增分配（MB）
    pub full_alloc_mb: f64,
    /// 生产路径单次构建累计新增分配（MB）
    pub window_alloc_mb: f64,
}

/// 病态锯齿规格（行段无法合并 → 覆盖格数的极端上界；厚度 20 时最坏）
fn zigzag(points: usize) -> Scenario {
    Scenario {
        name: "growth",
        points,
        step: 240.0,
        smooth_pitch: false,
        gated: true,
    }
}

/// 走真实交互链路的落笔（Pressed → Moved… → Released），返回全量覆盖格数
fn build_stroke(editor: &mut Editor, points: &[(f32, f32)], snap: f32) -> usize {
    let start = editor.line_pos_screen_pos(points[0]);
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(start.x, start.y),
        shift: false,
        ctrl: false,
    });
    for p in points.iter().skip(1) {
        let s = editor.line_pos_screen_pos(*p);
        editor.handle_action(EditorAction::Moved(Point2::new(s.x, s.y)));
    }
    editor.handle_action(EditorAction::Released);
    lumino_editor_state::brush_tool::cov::cover_cells(points, snap).len()
}

/// 曲线：不同笔画长度各测一次稳态每帧成本（旧口径 / 新口径同轮对比）
pub fn run_growth_curve(editor: &mut Editor, lens: &[usize], cycles: usize) -> Vec<GrowthRow> {
    let snap = editor.editor_state.view.snap_precision.max(1.0);
    let mut rows = Vec::new();
    for &n in lens {
        let spec = zigzag(n);
        let points = stroke_points(&spec);
        // 1px / 格：屏幕坐标与格一一对应（与 A/B 档同口径）
        editor.editor_state.view.zoom_x = 1.0 / spec.step.max(1.0);
        editor.editor_state.view.snap_precision = snap;

        let cells = build_stroke(editor, &points, snap);

        let mut full = Stat::new("旧口径");
        let mut canvas = Stat::new("画布几何");
        let mut window = Stat::new("wgpu 实例");
        let mut runs_full = 0usize;
        let mut runs_window = 0usize;
        for cycle in 0..=cycles {
            if cycle == 0 {
                let _ = preview_full_path(editor);
                let _ = preview_canvas_geometry(editor);
                let _ = preview_runs_and_rects(editor);
            } else {
                full.measure(|| runs_full = preview_full_path(editor));
                canvas.measure(|| {
                    let _ = preview_canvas_geometry(editor);
                });
                window.measure(|| runs_window = preview_runs_and_rects(editor));
            }
        }
        rows.push(GrowthRow {
            points: n,
            cells,
            runs_full,
            runs_window,
            full_ms: median(full.times()),
            canvas_ms: median(canvas.times()),
            window_ms: median(window.times()),
            full_alloc_mb: mb(full.max_alloc_growth()),
            window_alloc_mb: mb(window.max_alloc_growth()),
        });

        editor.handle_action(EditorAction::BrushCancel);
        let _ = lumino_message::events::take_events();
    }
    rows
}

/// 同一笔「边画边测」：画够 `marks` 个点时测一次当帧预览成本
///
/// 这是真实的掉帧曲线——同一笔从"流畅"到"卡顿"的拐点就在这里；
/// 修复后曲线应接近水平（成本只与视口内格数相关，与已画长度无关）。
pub fn run_growth_in_stroke(
    editor: &mut Editor,
    marks: &[usize],
    cycles: usize,
) -> Vec<(usize, f64)> {
    let snap = editor.editor_state.view.snap_precision.max(1.0);
    let total = marks.iter().copied().max().unwrap_or(0);
    let spec = zigzag(total);
    let points = stroke_points(&spec);
    editor.editor_state.view.zoom_x = 1.0 / spec.step.max(1.0);
    editor.editor_state.view.snap_precision = snap;

    let start = editor.line_pos_screen_pos(points[0]);
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(start.x, start.y),
        shift: false,
        ctrl: false,
    });

    let mut out = Vec::new();
    let mut pushed = 1usize;
    for &mark in marks {
        while pushed < mark {
            let s = editor.line_pos_screen_pos(points[pushed]);
            editor.handle_action(EditorAction::Moved(Point2::new(s.x, s.y)));
            pushed += 1;
        }
        let mut stat = Stat::new("当帧预览");
        for cycle in 0..=cycles {
            if cycle == 0 {
                let _ = preview_runs_and_rects(editor);
            } else {
                stat.measure(|| {
                    let _ = preview_runs_and_rects(editor);
                });
            }
        }
        out.push((mark, median(stat.times())));
    }

    editor.handle_action(EditorAction::Released);
    editor.handle_action(EditorAction::BrushCancel);
    let _ = lumino_message::events::take_events();
    out
}

/// 打印曲线表 + 判定（返回是否全部达标）
pub fn report_growth(rows: &[GrowthRow], thickness: u8, target_ms: f64) -> bool {
    println!(
        "── C 增长曲线：每帧预览成本 vs 笔画长度（病态锯齿 / 粗细度 {thickness} / \
         新口径线 {target_ms:.1}ms） ──"
    );
    println!(
        "{:<9} | {:<9} | {:<10} | {:<11} | {:<10} | {:<10} | {:<10} | {:<10} | 判定",
        "点数",
        "覆盖格",
        "行段(全量)",
        "实例(可见)",
        "全量画布(ms)",
        "画布几何(ms)",
        "wgpu实例(ms)",
        "实例分配(MB)"
    );
    let mut pass = true;
    for row in rows {
        let ok = row.window_ms <= target_ms;
        pass &= ok;
        let verdict = if !ok {
            "✗ 超预览线"
        } else if row.window_ms > FRAME_BUDGET_MS {
            "✗ 掉帧"
        } else if row.window_ms > FRAME_BUDGET_MS * 0.5 {
            "△ 接近帧预算"
        } else {
            "✓"
        };
        println!(
            "{:<9} | {:<9} | {:<10} | {:<11} | {:<10.3} | {:<10.3} | {:<10.3} | {:<10.2} | {}",
            row.points,
            row.cells,
            row.runs_full,
            row.runs_window,
            row.full_ms,
            row.canvas_ms,
            row.window_ms,
            row.window_alloc_mb,
            verdict
        );
    }
    println!(
        "  注：三条路径均**不含** iced canvas 的 lyon 细分与逐块 `Frame::fill` —— \
         真机 puffin 实测 8 万方块单帧 81.6ms（§18 改走 wgpu 的直接原因）"
    );
    if let (Some(first), Some(last)) = (rows.first(), rows.last()) {
        println!(
            "  成本增长倍数（{} → {} 点）: 全量画布 {:.1}× / 画布几何 {:.1}× / wgpu 实例 {:.1}×",
            first.points,
            last.points,
            if first.full_ms > 0.0 {
                last.full_ms / first.full_ms
            } else {
                0.0
            },
            if first.canvas_ms > 0.0 {
                last.canvas_ms / first.canvas_ms
            } else {
                0.0
            },
            if first.window_ms > 0.0 {
                last.window_ms / first.window_ms
            } else {
                0.0
            }
        );
        println!(
            "  提速（{} 点）: 全量 {:.1}× / 几何 {:.1}× | 每帧分配 {:.1}MB → {:.1}MB | 方块数 {} → {}（视口外 {:.2}% 不再计算）",
            last.points,
            if last.window_ms > 0.0 {
                last.full_ms / last.window_ms
            } else {
                0.0
            },
            if last.window_ms > 0.0 {
                last.canvas_ms / last.window_ms
            } else {
                0.0
            },
            last.full_alloc_mb,
            last.window_alloc_mb,
            last.runs_full,
            last.runs_window,
            if last.runs_full > 0 {
                (1.0 - last.runs_window as f64 / last.runs_full as f64) * 100.0
            } else {
                0.0
            }
        );
    }
    pass
}

/// 打印「同一笔边画边测」曲线
pub fn report_in_stroke(curve: &[(usize, f64)], target_ms: f64) -> bool {
    println!();
    println!("── C2 同一笔边画边测：当帧预览成本（掉帧真实曲线） ──");
    let mut pass = true;
    for (points, ms) in curve {
        let ok = *ms <= target_ms;
        pass &= ok;
        println!(
            "  已画 {:<7} 点 | 当帧 {:<9.3} ms | {}{}",
            points,
            ms,
            if *ms > FRAME_BUDGET_MS {
                "✗ 掉帧（> 16.67ms）"
            } else if ok {
                "✓"
            } else {
                "✗ 超预览线"
            },
            if *ms > FRAME_BUDGET_MS * 0.5 {
                " ← 已接近/超过帧预算的一半"
            } else {
                ""
            }
        );
    }
    pass
}
