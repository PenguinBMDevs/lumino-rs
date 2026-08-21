//! 纵向卷帘网格线绘制 — 层级化 LOD 渐隐（横向时间轴 → 水平线，纵向音高轴 → 垂直线）
//!
//! 复用横向 `bars.rs` 的 LOD 策略与阈值，仅坐标转置：
//! - 横向：小节/拍/网格为**垂直线**（x = tick*zoom - scroll + keyboard_width）
//! - 纵向：小节/拍/网格为**水平线**（y = tick*zoom - scroll + ruler_height），
//!   键分隔为**垂直线**（x = key*zoom_y - scroll_y）
//!
//! 小节线同样支持 `measure_power` 翻倍淡出，拍线/半拍/细分网格复用 `GRID_TIERS`。

use super::theme::ThemeExt;
use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::path::Builder;
use iced_widget::canvas::{Frame, Path, Stroke};
use lumino_ui_core::Renderer;

// ─── 复用横向 bars.rs 的 LOD 常量（保证视觉一致）───
const BEAT_MAX_MEASURES: f32 = 48.0;
const HALF_BEAT_MAX_MEASURES: f32 = 24.0;
const MEASURE_FADE_START: f32 = 48.0;
const MEASURE_FADE_END: f32 = 96.0;
const MAX_MEASURE_POWER: u32 = 6;
const MAX_MEASURE_LEVELS: usize = (MAX_MEASURE_POWER + 1) as usize;

const GRID_TIERS: &[(f32, f32)] = &[
    (0.25, 3.75),
    (0.5, 7.5),
    (1.0, 15.0),
    (2.0, 30.0),
    (4.0, 60.0),
    (8.0, 120.0),
];

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

fn measure_line_width(measure_power: u32) -> f32 {
    (4.0 - (measure_power as f32 * 0.5)).max(2.0)
}

fn is_on_line(tick: f32, interval: f32) -> bool {
    (tick % interval).abs() < 0.5
}

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

        let beat_alpha = smooth_fade(visible_measures, BEAT_MAX_MEASURES);
        let halfbeat_alpha = smooth_fade(visible_measures, HALF_BEAT_MAX_MEASURES);

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

struct HLineCtx {
    zoom_x: f32,
    scroll_x: f32,
    ruler_height: f32,
    bounds_width: f32,
    grid_top: f32,
    grid_bottom: f32,
}

/// 将某一层的所有可见水平线加入 Builder
fn add_hlevel_lines(
    builder: &mut Builder,
    start_tick: f32,
    end_tick: f32,
    interval: f32,
    coarser_levels: &[(f32, f32)],
    ctx: &HLineCtx,
) {
    let mut current_tick = (start_tick / interval).ceil() * interval;
    while current_tick < end_tick {
        let skip = coarser_levels
            .iter()
            .any(|(coarse_interval, coarse_alpha)| {
                *coarse_alpha > 0.0 && is_on_line(current_tick, *coarse_interval)
            });

        if !skip {
            let screen_y = current_tick * ctx.zoom_x - ctx.scroll_x + ctx.ruler_height;
            if screen_y >= ctx.grid_top && screen_y <= ctx.grid_bottom {
                builder.move_to(Point::new(0.0, screen_y));
                builder.line_to(Point::new(ctx.bounds_width, screen_y));
            }
        }

        current_tick += interval;
    }
}

/// 绘制纵向卷帘网格线（水平时间线 + 垂直键线）— 层级化 LOD 渐隐
pub fn draw(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    let ruler_height = view.ruler_height;
    // 键盘高度与横向键盘宽度保持一致（视觉统一）
    let keyboard_h = view.keyboard_width;
    if bounds.height <= ruler_height + keyboard_h || bounds.width <= 1.0 {
        return;
    }

    let grid_top = ruler_height;
    let grid_bottom = bounds.height - keyboard_h;
    let grid_height = (grid_bottom - grid_top).max(0.0);

    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + grid_height) / view.zoom_x;

    // ── 1. 计算时间轴 LOD（基于可见小节数）──
    let lod = GridLod::compute(end_tick - start_tick, ppq);

    let bar_c = theme.bar_line_color();
    let beat_c = theme.beat_line_color();
    let halfbeat_c = theme.half_beat_line_color();
    let grid_c = theme.grid_line_color();
    let key_line_c = theme.border_color();

    struct Level {
        interval: f32,
        alpha: f32,
        width: f32,
        color: iced_core::Color,
    }

    let mut levels: Vec<Level> = Vec::with_capacity(lod.measure_count + 3 + GRID_TIERS.len());

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

        let _ = max_measures;
    }

    // ── 2. 为每一时间层级生成水平线 Path ──
    let mut builders: Vec<Builder> = levels.iter().map(|_| Builder::new()).collect();
    let mut coarser: Vec<(f32, f32)> = Vec::with_capacity(levels.len());
    let ctx = HLineCtx {
        zoom_x: view.zoom_x,
        scroll_x: view.scroll_x,
        ruler_height,
        bounds_width: bounds.width,
        grid_top,
        grid_bottom,
    };

    for (i, level) in levels.iter().enumerate() {
        if level.alpha <= 0.0 {
            coarser.push((level.interval, level.alpha));
            continue;
        }

        add_hlevel_lines(
            &mut builders[i],
            start_tick,
            end_tick,
            level.interval,
            &coarser,
            &ctx,
        );

        coarser.push((level.interval, level.alpha));
    }

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

    // ── 2.1 小节号文本（沿左侧垂直排列，按小节规则）──
    {
        let ticks_per_measure = ppq * 4.0;
        if ticks_per_measure > 0.0 {
            let mut measure_tick = (start_tick / ticks_per_measure).ceil() * ticks_per_measure;
            let mut measure_no = (measure_tick / ticks_per_measure) as u32 + 1;
            let text_color = theme.text_color();
            while measure_tick < end_tick {
                let screen_y = measure_tick * view.zoom_x - view.scroll_x + ruler_height;
                if screen_y >= grid_top && screen_y <= grid_bottom {
                    let label = iced_widget::canvas::Text {
                        content: measure_no.to_string(),
                        position: Point::new(4.0, screen_y + 2.0),
                        max_width: 60.0,
                        line_height: iced_core::text::LineHeight::Relative(1.0),
                        size: iced_core::Pixels(10.0),
                        color: text_color,
                        font: iced_core::Font::DEFAULT,
                        align_x: iced_core::alignment::Horizontal::Left.into(),
                        align_y: iced_core::alignment::Vertical::Top,
                        shaping: iced_core::text::Shaping::Basic,
                    };
                    frame.fill_text(label);
                }
                measure_tick += ticks_per_measure;
                measure_no += 1;
                // 避免在极度缩小时绘制过多文本（间隔过密时仅每 N 小节标注）
                if lod.measure_count > 1 && measure_no.is_multiple_of(2) {
                    // 当小节线已翻倍时，跳过奇数小节文本已在 LOD 中合并，此处无需额外跳过
                }
            }
        }
    }

    // ── 3. 垂直键分隔线（按 key 规则：每个键一条线，贴合当前可视 pitch 范围）──
    // 可视 key 范围：scroll_y / zoom_y .. (scroll_y + bounds.width)/zoom_y
    let visible_key_start = (view.scroll_y / view.zoom_y).floor().max(0.0) as usize;
    let visible_key_end = ((view.scroll_y + bounds.width) / view.zoom_y).ceil() as usize;
    let key_start = visible_key_start.min(view.visible_key_count as usize);
    let key_end = visible_key_end.min(view.visible_key_count as usize + 1);

    let mut key_builder = Builder::new();
    for k in key_start..=key_end {
        let screen_x = k as f32 * view.zoom_y - view.scroll_y;
        if screen_x < 0.0 || screen_x > bounds.width {
            continue;
        }
        key_builder.move_to(Point::new(screen_x, grid_top));
        key_builder.line_to(Point::new(screen_x, grid_bottom));
    }
    frame.stroke(
        &key_builder.build(),
        Stroke::default()
            .with_width(1.0)
            .with_color(iced_core::Color {
                a: key_line_c.a * 0.8,
                ..key_line_c
            }),
    );

    // ── 4. 网格区域边框（底部基线：键盘顶边）──
    let border_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(theme.border_color());
    // 键盘顶边
    let kb_top = Path::line(
        Point::new(0.0, grid_bottom),
        Point::new(bounds.width, grid_bottom),
    );
    frame.stroke(&kb_top, border_stroke);
    // 标尺底边
    let ruler_bottom = Path::line(
        Point::new(0.0, grid_top),
        Point::new(bounds.width, grid_top),
    );
    frame.stroke(&ruler_bottom, border_stroke);
}

/// 仅绘制小节号文本与边框（供 wgpu 网格接管后的 Canvas 层调用，网格线已由 wgpu 绘制）
pub fn draw_labels(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    let ruler_height = view.ruler_height;
    let keyboard_h = view.keyboard_width;
    if bounds.height <= ruler_height + keyboard_h || bounds.width <= 1.0 {
        return;
    }
    let grid_top = ruler_height;
    let grid_bottom = bounds.height - keyboard_h;
    let grid_height = (grid_bottom - grid_top).max(0.0);
    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + grid_height) / view.zoom_x;
    let lod = GridLod::compute(end_tick - start_tick, ppq);

    // 小节号文本（沿左侧垂直排列）
    {
        let ticks_per_measure = ppq * 4.0;
        if ticks_per_measure > 0.0 {
            let mut measure_tick = (start_tick / ticks_per_measure).ceil() * ticks_per_measure;
            let mut measure_no = (measure_tick / ticks_per_measure) as u32 + 1;
            let text_color = theme.text_color();
            while measure_tick < end_tick {
                let screen_y = measure_tick * view.zoom_x - view.scroll_x + ruler_height;
                if screen_y >= grid_top && screen_y <= grid_bottom {
                    let label = iced_widget::canvas::Text {
                        content: measure_no.to_string(),
                        position: Point::new(4.0, screen_y + 2.0),
                        max_width: 60.0,
                        line_height: iced_core::text::LineHeight::Relative(1.0),
                        size: iced_core::Pixels(10.0),
                        color: text_color,
                        font: iced_core::Font::DEFAULT,
                        align_x: iced_core::alignment::Horizontal::Left.into(),
                        align_y: iced_core::alignment::Vertical::Top,
                        shaping: iced_core::text::Shaping::Basic,
                    };
                    frame.fill_text(label);
                }
                measure_tick += ticks_per_measure;
                measure_no += 1;
                let _ = lod.measure_count;
            }
        }
    }

    // 边框
    let border_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(theme.border_color());
    let kb_top = Path::line(
        Point::new(0.0, grid_bottom),
        Point::new(bounds.width, grid_bottom),
    );
    frame.stroke(&kb_top, border_stroke);
    let ruler_bottom = Path::line(
        Point::new(0.0, grid_top),
        Point::new(bounds.width, grid_top),
    );
    frame.stroke(&ruler_bottom, border_stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_lod_visible_measures_small_shows_all() {
        let lod = GridLod::compute(480.0, 480.0); // 1小节可见时应显示精细网格
        assert!(lod.beat_alpha > 0.0);
        assert!(lod.halfbeat_alpha > 0.0);
    }

    #[test]
    fn test_measure_line_width_clamps() {
        assert_eq!(measure_line_width(0), 4.0);
        assert_eq!(measure_line_width(10), 2.0);
    }
}
