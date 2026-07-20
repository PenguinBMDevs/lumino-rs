//! 小节线/网格线绘制 — LOD（Level of Detail）平滑缩放
//!
//! # 算法
//!
//! 以**视口内可见小节数**为核心判断依据，计算每种线型是否显示。
//! 可见小节数越少（放大）→ 线型越细 → 网格越密。
//! 可见小节数越多（缩小）→ 线型越粗 → 网格越疏直至消失。
//!
//! 四种线型（小节线/拍线/半拍线/网格线）在一次遍历中按音乐层级决定。
//! 多小节是"小节线可见间隔自适应翻倍"，不是独立层级。
//!
//! 核心逻辑位于 [`compute_grid_lod`]，所有三个渲染路径共享同一算法。

use super::theme::ThemeExt;
use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::path::Builder;
use iced_widget::canvas::{Frame, Path, Stroke};
use lumino_ui_core::Renderer;

// ─── LOD 阈值（基于可见小节数）───

/// 网格线分档（可见小节数 → 最细网格间隔）
/// 每档产生 ~192 线/视口（measures_per_viewport × ticks_per_measure / interval）。
const GRID_TIERS: &[(f32, f32)] = &[
    (0.375, 3.75), // 512分音符（beat/128）
    (0.75, 7.5),   // 256分音符（beat/64）
    (1.5, 15.0),   // 128分音符（beat/32）
    (3.0, 30.0),   // 64分音符（beat/16）
    (6.0, 60.0),   // 32分音符（beat/8）
    (12.0, 120.0), // 16分音符（beat/4）
];

/// 半拍线（8分音符）可见上限：< 24 小节
const HALFBEAT_MAX_MEASURES: f32 = 24.0;
/// 拍线（4分音符）可见上限：< 48 小节
const BEAT_MAX_MEASURES: f32 = 48.0;

/// 小节间隔自适应：保持 ~48 个小节标记（缩放时线数不变）
const MEASURE_TARGET_COUNT: f32 = 48.0;
/// 小节间隔最大翻倍次数（2^6 = 64 小节间隔）
const MAX_MEASURE_POWER: u32 = 6;

// ─── LOD 辅助函数 ───

/// 线型 alpha 淡入（基于可见小节数离阈值的距离）
fn alpha_fade(visible_measures: f32, max_measures: f32) -> f32 {
    if visible_measures >= max_measures {
        return 0.0;
    }
    // 在 0 → max_measures 范围，alpha 从 1.0 → 0.0 平滑过渡
    // 使用对数比例让过渡更自然
    let t = 1.0 - (visible_measures / max_measures);
    let t = t * t; // 平方缓出
    0.3 + 0.7 * t
}

/// 小节线宽自适应
fn measure_line_width(measure_power: u32) -> f32 {
    // power 0 → 4.0, power 1 → 3.5, power 2 → 3.0, ... power 4+ → 2.0
    (4.0 - (measure_power as f32 * 0.5).min(2.0)).max(2.0)
}

/// 计算 LOD 参数（共享给 bars / editor_impl / gfx grid）
pub fn compute_grid_lod(
    visible_tick_range: f32,
    ppq: f32,
) -> (f32, f32, bool, bool, Option<f32>, f32, f32, f32, f32) {
    let ticks_per_measure = ppq * 4.0;
    let visible_measures = visible_tick_range / ticks_per_measure;

    // ── 1. 小节间隔自适应翻倍 ──
    let measure_power = if visible_measures > MEASURE_TARGET_COUNT {
        (visible_measures / MEASURE_TARGET_COUNT).log2().ceil() as u32
    } else {
        0
    };
    let measure_power = measure_power.min(MAX_MEASURE_POWER);
    let measure_int = ticks_per_measure * (1u32 << measure_power) as f32;
    let measure_width = measure_line_width(measure_power);

    // ── 2. 拍线 / 半拍线可见性 ──
    let show_beats = visible_measures < BEAT_MAX_MEASURES;
    let show_halfbeats = visible_measures < HALFBEAT_MAX_MEASURES;

    // ── 3. 网格线 ──
    let finest_grid = GRID_TIERS
        .iter()
        .find(|(max_measures, _)| visible_measures <= *max_measures)
        .map(|(_, interval)| *interval);

    // ── 4. alpha ──
    // 小节线始终有透明度（基于 measure_power）
    let measure_alpha = if measure_power == 0 {
        1.0
    } else {
        0.7 + 0.3 * (1.0 - (measure_power as f32 / MAX_MEASURE_POWER as f32))
    };

    let beat_alpha = if show_beats {
        alpha_fade(visible_measures, BEAT_MAX_MEASURES)
    } else {
        0.0
    };
    let halfbeat_alpha = if show_halfbeats {
        alpha_fade(visible_measures, HALFBEAT_MAX_MEASURES)
    } else {
        0.0
    };
    let grid_alpha = if let Some(g) = finest_grid {
        // 查找阈值
        let max_m = GRID_TIERS
            .iter()
            .find(|(_, interval)| *interval == g)
            .map(|(m, _)| *m)
            .unwrap_or(64.0);
        alpha_fade(visible_measures, max_m)
    } else {
        0.0
    };

    (
        measure_int,
        measure_width,
        show_beats,
        show_halfbeats,
        finest_grid,
        measure_alpha,
        beat_alpha,
        halfbeat_alpha,
        grid_alpha,
    )
}

/// 绘制小节线和拍线（纵向线）— LOD 平滑缩放
pub fn draw(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;
    let zoom_x = view.zoom_x;

    let start_tick = view.scroll_x / zoom_x;
    let end_tick = (view.scroll_x + bounds.width - keyboard_width) / zoom_x;
    let tick_range = end_tick - start_tick;

    // ── 1. 计算 LOD ──
    let (
        measure_int,
        measure_width,
        show_beats,
        show_halfbeats,
        finest_grid,
        measure_alpha,
        beat_alpha,
        halfbeat_alpha,
        grid_alpha,
    ) = compute_grid_lod(tick_range, ppq);

    // ── 2. 确定迭代步长 ──
    let step = finest_grid.unwrap_or(if show_halfbeats {
        ppq / 2.0
    } else if show_beats {
        ppq
    } else {
        measure_int
    });

    // ── 3. 获取主题色 ──
    let bar_c = theme.bar_line_color();
    let beat_c = theme.beat_line_color();
    let halfbeat_c = theme.half_beat_line_color();
    let grid_c = theme.grid_line_color();

    let bar_c = iced_core::Color {
        a: bar_c.a * measure_alpha,
        ..bar_c
    };
    let beat_c = iced_core::Color {
        a: beat_c.a * beat_alpha,
        ..beat_c
    };
    let halfbeat_c = iced_core::Color {
        a: halfbeat_c.a * halfbeat_alpha,
        ..halfbeat_c
    };
    let grid_c = iced_core::Color {
        a: grid_c.a * grid_alpha,
        ..grid_c
    };

    // ── 4. 一次遍历，音乐层级决定线型 ──
    let mut bar_builder = Builder::new();
    let mut beat_builder = Builder::new();
    let mut halfbeat_builder = Builder::new();
    let mut grid_builder = Builder::new();

    let mut current_tick = (start_tick / step).ceil() * step;
    while current_tick < end_tick {
        let screen_x = (current_tick * zoom_x) - view.scroll_x + keyboard_width;

        if screen_x >= keyboard_width && screen_x <= bounds.width {
            let is_measure = (current_tick % measure_int).abs() < 0.5;
            let is_beat = show_beats && (current_tick % ppq).abs() < 0.5;
            let is_halfbeat = show_halfbeats && (current_tick % (ppq / 2.0)).abs() < 0.5;

            if is_measure {
                bar_builder.move_to(Point::new(screen_x, ruler_height));
                bar_builder.line_to(Point::new(screen_x, bounds.height));
            } else if is_beat {
                beat_builder.move_to(Point::new(screen_x, ruler_height));
                beat_builder.line_to(Point::new(screen_x, bounds.height));
            } else if is_halfbeat {
                halfbeat_builder.move_to(Point::new(screen_x, ruler_height));
                halfbeat_builder.line_to(Point::new(screen_x, bounds.height));
            } else if finest_grid.is_some() {
                grid_builder.move_to(Point::new(screen_x, ruler_height));
                grid_builder.line_to(Point::new(screen_x, bounds.height));
            }
        }
        current_tick += step;
    }

    // ── 5. 批量绘制（从粗到细）──
    frame.stroke(
        &bar_builder.build(),
        Stroke::default()
            .with_width(measure_width)
            .with_color(bar_c),
    );
    frame.stroke(
        &beat_builder.build(),
        Stroke::default().with_width(1.5).with_color(beat_c),
    );
    frame.stroke(
        &halfbeat_builder.build(),
        Stroke::default().with_width(1.0).with_color(halfbeat_c),
    );
    frame.stroke(
        &grid_builder.build(),
        Stroke::default().with_width(0.5).with_color(grid_c),
    );

    // 底部基线
    let baseline_stroke = Stroke::default()
        .with_width(2.0)
        .with_color(theme.border_color());
    let baseline = Path::line(
        Point::new(keyboard_width, bounds.height),
        Point::new(bounds.width, bounds.height),
    );
    frame.stroke(&baseline, baseline_stroke);
}
