//! 小节线/网格线绘制 — 层级化 LOD 渐隐
//!
//! # 核心思想
//!
//! 纵向线按音乐层级组织（粗 → 细）：
//!
//! ```text
//! 小节线 > 拍线（4分音符） > 半拍线（8分音符） > 16分网格 > ... > 512分网格
//! ```
//!
//! 缩放缩小时，**永远是最细的层级先淡出**；如果一条细线恰好落在更粗的层级上，
//! 它由更粗的层级绘制，从而保持可见。
//!
//! 例如当视口最细显示到八分音符线时：
//! - 八分音符线里属于四分音符的位置，继续按四分音符绘制；
//! - 其余八分音符线才参与八分音符层的淡出。
//!
//! 小节线同样使用该淡出曲线：当基础小节间隔太密时，通过 `measure_power`
//! 让间隔翻倍，并在每个 power 内保持 `[fade_start, fade_end]` 的连续淡出。

use super::theme::ThemeExt;
use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::path::Builder;
use iced_widget::canvas::{Frame, Path, Stroke};
use lumino_ui_core::Renderer;

// ─── LOD 阈值（基于视口内可见小节数）───

/// 拍线（4分音符）完全消失阈值
const BEAT_MAX_MEASURES: f32 = 48.0;
/// 半拍线（8分音符）完全消失阈值
const HALF_BEAT_MAX_MEASURES: f32 = 24.0;

/// 小节线每个 power 的淡出起始可见小节数
const MEASURE_FADE_START: f32 = 48.0;
/// 小节线每个 power 的淡出结束可见小节数
const MEASURE_FADE_END: f32 = 96.0;
/// 小节间隔最大翻倍次数（2^6 = 64 小节间隔）
const MAX_MEASURE_POWER: u32 = 6;
/// 小节 LOD 层级数量
const MAX_MEASURE_LEVELS: usize = (MAX_MEASURE_POWER + 1) as usize;

/// 细分网格层级（从最细到最粗）。
/// 元组为 `(max_measures, interval_ticks)`：
/// - `max_measures`：该层级完全消失时的可见小节数
/// - `interval_ticks`：该层级的 tick 间隔
const GRID_TIERS: &[(f32, f32)] = &[
    (0.25, 3.75), // 512分音符（beat/128）
    (0.5, 7.5),   // 256分音符（beat/64）
    (1.0, 15.0),  // 128分音符（beat/32）
    (2.0, 30.0),  // 64分音符（beat/16）
    (4.0, 60.0),  // 32分音符（beat/8）
    (8.0, 120.0), // 16分音符（beat/4）
];

// ─── LOD 辅助函数 ───

/// 在 `[max/2, max]` 区间内从 1.0 平滑淡出到 0.0。
fn smooth_fade(visible_measures: f32, max_measures: f32) -> f32 {
    if visible_measures >= max_measures {
        return 0.0;
    }
    let start = max_measures * 0.5;
    if visible_measures <= start {
        return 1.0;
    }
    let t = (visible_measures - start) / (max_measures - start);
    1.0 - t * t
}

/// 在任意 `[start, end]` 区间内从 1.0 平滑淡出到 0.0。
fn smooth_fade_range(value: f32, start: f32, end: f32) -> f32 {
    if value <= start {
        return 1.0;
    }
    if value >= end {
        return 0.0;
    }
    let t = (value - start) / (end - start);
    1.0 - t * t
}

/// 小节线宽自适应
fn measure_line_width(measure_power: u32) -> f32 {
    // power 0 → 4.0, power 1 → 3.5, ... power 4+ → 2.0
    (4.0 - (measure_power as f32 * 0.5)).max(2.0)
}

/// 判断 `tick` 是否落在某条粗线上（允许浮点误差）。
fn is_on_line(tick: f32, interval: f32) -> bool {
    (tick % interval).abs() < 0.5
}

/// 单个小节线 LOD 层级
#[derive(Debug, Clone, Copy)]
struct MeasureLod {
    interval: f32,
    alpha: f32,
    width: f32,
}

impl MeasureLod {
    const DEFAULT: Self = Self {
        interval: 0.0,
        alpha: 0.0,
        width: 0.0,
    };
}

/// 单层 LOD 参数
#[derive(Debug, Clone, Copy)]
struct GridLod {
    measures: [MeasureLod; MAX_MEASURE_LEVELS],
    measure_count: usize,
    beat_alpha: f32,
    halfbeat_alpha: f32,
    grid_alphas: [f32; GRID_TIERS.len()],
}

impl GridLod {
    fn compute(visible_tick_range: f32, ppq: f32) -> Self {
        let ticks_per_measure = ppq * 4.0;
        let visible_measures = visible_tick_range / ticks_per_measure;

        // ── 小节线：每个 power 独立计算 alpha，粗线优先绘制 ──
        // 这样可以保证：当细密的小节线淡出时，更粗的小节线已经可见，
        // 不会出现“所有小节线一起消失”的断层。
        let mut measures = [MeasureLod::DEFAULT; MAX_MEASURE_LEVELS];
        let mut measure_count = 0;
        for power in 0..=MAX_MEASURE_POWER {
            let fade_start = MEASURE_FADE_START * (1u32 << power) as f32;
            let fade_end = MEASURE_FADE_END * (1u32 << power) as f32;
            let mut alpha = smooth_fade_range(visible_measures, fade_start, fade_end);
            if power > 0 && visible_measures <= fade_start / 2.0 {
                alpha = 0.0;
            }
            if alpha > 0.0 {
                measures[measure_count] = MeasureLod {
                    interval: ticks_per_measure * (1u32 << power) as f32,
                    alpha,
                    width: measure_line_width(power),
                };
                measure_count += 1;
            }
        }

        // ── 拍线 / 半拍线 ──
        let beat_alpha = smooth_fade(visible_measures, BEAT_MAX_MEASURES);
        let halfbeat_alpha = smooth_fade(visible_measures, HALF_BEAT_MAX_MEASURES);

        // ── 细分网格 ──
        let mut grid_alphas = [0.0; GRID_TIERS.len()];
        for (i, (max_measures, _)) in GRID_TIERS.iter().enumerate() {
            grid_alphas[i] = smooth_fade(visible_measures, *max_measures);
        }

        Self {
            measures,
            measure_count,
            beat_alpha,
            halfbeat_alpha,
            grid_alphas,
        }
    }
}

/// 线段绘制所需的视口/缩放上下文。
struct LineDrawCtx {
    zoom_x: f32,
    scroll_x: f32,
    keyboard_width: f32,
    bounds_width: f32,
    y0: f32,
    y1: f32,
}

/// 把某一层的所有可见线段加入对应 Builder。
///
/// 若 `tick` 同时落在某个已处理的更粗层级上，则跳过，避免同一点被多次绘制。
fn add_level_lines(
    builder: &mut Builder,
    start_tick: f32,
    end_tick: f32,
    interval: f32,
    coarser_levels: &[(f32, f32)],
    ctx: &LineDrawCtx,
) {
    let mut current_tick = (start_tick / interval).ceil() * interval;
    while current_tick < end_tick {
        let skip = coarser_levels
            .iter()
            .any(|(coarse_interval, coarse_alpha)| {
                *coarse_alpha > 0.0 && is_on_line(current_tick, *coarse_interval)
            });

        if !skip {
            let screen_x = current_tick * ctx.zoom_x - ctx.scroll_x + ctx.keyboard_width;
            if screen_x >= ctx.keyboard_width && screen_x <= ctx.bounds_width {
                builder.move_to(Point::new(screen_x, ctx.y0));
                builder.line_to(Point::new(screen_x, ctx.y1));
            }
        }

        current_tick += interval;
    }
}

/// 绘制小节线和拍线（纵向线）— 层级化 LOD 渐隐
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
    let scroll_x = view.scroll_x;

    let start_tick = scroll_x / zoom_x;
    let end_tick = (scroll_x + bounds.width - keyboard_width) / zoom_x;

    // ── 1. 计算 LOD ──
    let lod = GridLod::compute(end_tick - start_tick, ppq);

    // ── 2. 准备层级（从粗到细）──
    let bar_c = theme.bar_line_color();
    let beat_c = theme.beat_line_color();
    let halfbeat_c = theme.half_beat_line_color();
    let grid_c = theme.grid_line_color();

    struct Level {
        interval: f32,
        alpha: f32,
        width: f32,
        color: iced_core::Color,
    }

    let mut levels: Vec<Level> = Vec::with_capacity(lod.measure_count + 3 + GRID_TIERS.len());

    // 小节线层级从粗到细加入（measure_count-1 是最粗的 power）
    for i in (0..lod.measure_count).rev() {
        let m = lod.measures[i];
        levels.push(Level {
            interval: m.interval,
            alpha: m.alpha,
            width: m.width,
            color: iced_core::Color {
                a: bar_c.a * m.alpha,
                ..bar_c
            },
        });
    }

    levels.push(Level {
        interval: ppq,
        alpha: lod.beat_alpha,
        width: 1.5,
        color: iced_core::Color {
            a: beat_c.a * lod.beat_alpha,
            ..beat_c
        },
    });
    levels.push(Level {
        interval: ppq / 2.0,
        alpha: lod.halfbeat_alpha,
        width: 1.0,
        color: iced_core::Color {
            a: halfbeat_c.a * lod.halfbeat_alpha,
            ..halfbeat_c
        },
    });

    // 网格层级按从粗到细加入（GRID_TIERS 原始顺序为从细到粗，故反向遍历）
    for i in (0..GRID_TIERS.len()).rev() {
        let (max_measures, interval) = GRID_TIERS[i];
        let alpha = lod.grid_alphas[i];
        let fineness = (GRID_TIERS.len() - 1 - i) as f32;
        let width = (0.5 - fineness * 0.05).max(0.25);

        levels.push(Level {
            interval,
            alpha,
            width,
            color: iced_core::Color {
                a: grid_c.a * alpha,
                ..grid_c
            },
        });

        // 避免编译器/ clippy 对未使用 `max_measures` 的警告。
        let _ = max_measures;
    }

    // ── 3. 为每一层生成 Path ──
    let mut builders: Vec<Builder> = levels.iter().map(|_| Builder::new()).collect();
    let mut coarser: Vec<(f32, f32)> = Vec::with_capacity(levels.len());
    let ctx = LineDrawCtx {
        zoom_x,
        scroll_x,
        keyboard_width,
        bounds_width: bounds.width,
        y0: ruler_height,
        y1: bounds.height,
    };

    for (i, level) in levels.iter().enumerate() {
        if level.alpha <= 0.0 {
            coarser.push((level.interval, level.alpha));
            continue;
        }

        add_level_lines(
            &mut builders[i],
            start_tick,
            end_tick,
            level.interval,
            &coarser,
            &ctx,
        );

        coarser.push((level.interval, level.alpha));
    }

    // ── 4. 批量绘制（从粗到细，但因已做层级剔除，顺序不影响像素）──
    for (builder, level) in builders.into_iter().zip(levels) {
        if level.alpha <= 0.0 {
            continue;
        }
        frame.stroke(
            &builder.build(),
            Stroke::default()
                .with_width(level.width)
                .with_color(level.color),
        );
    }

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

#[cfg(test)]
mod tests;
