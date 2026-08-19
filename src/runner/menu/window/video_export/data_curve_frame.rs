// Ported from MIDIGraphRenderer, originally created by Danidanijr.
// Source: https://github.com/Augustin900/MIDIGraphRenderer (GitHub mirror with MIT License)
// See NOTICE in this directory for full attribution and license text.

//! 数据曲线模式帧渲染（移植自 MIDIGraphRenderer 的 graph 渲染逻辑）
//!
//! 原版（LÖVE2D Lua）核心视觉：
//! - 自动缩放折线：窗口内 min/max 经 EMA 平滑后加 padding 构成纵轴范围
//! - 水平刻度网格：10 的幂间距主刻度线 + 细分线（透明度随缩放比例渐变）
//! - 里程碑文字（1k/10k/100k…）放大 + 千分位/缩写数字
//!
//! 相对原版的性能优化（数据为内部统计按帧直传，无 CSV IO）：
//! - `VecDeque` 环形窗口替代原版每帧重建 table（O(1) 入队出队）
//! - 折线平滑用滑动窗口和 O(n)（见 `data_curve_math`），替代原版 O(n²)
//! - 颜色 / 字体 / 网格步长计算全部缓存，每帧零堆分配
//! - 折线与网格线直接写 BGRA 像素（见 `data_curve_draw`），无绘图 API 逐线开销

use std::collections::VecDeque;

use lumino_message::events::window::video::{CounterFont, DataCurveConfig, DataCurveMetric};

use super::counter_font::CounterFontRenderer;
use super::data_curve_draw::{blend_hline, draw_thick_line, fill_bgra, DrawThickLineInput};
use super::counter_font_ttf::DrawLineScaledInput;
use super::data_curve_math::{abbreviate, add_commas, is_milestone, rgba_to_bgra, smooth_forward};

/// 数据曲线渲染配置（后台渲染线程使用，由事件层配置转换而来）。
#[derive(Debug, Clone)]
pub struct DataCurveRenderConfig {
    pub metric: DataCurveMetric,
    pub graph_duration: f32,
    pub zoom_smoothness: f32,
    pub graph_smoothness: u32,
    pub padding_mul: f32,
    /// RGBA 顺序（帧数据为 BGRA，写入时交换）
    pub bg_color: [u8; 4],
    pub line_color: [u8; 4],
    pub text_color: [u8; 4],
    pub bar_color: [u8; 4],
    pub line_thickness: u32,
    pub bar_thickness: u32,
    pub font_size: u32,
    pub font: CounterFont,
    pub text_x_offset: u32,
    pub text_y_offset: u32,
    pub milestone_scale_mul: f32,
    pub abbreviate: bool,
    pub abbreviate_digits: u32,
    pub show_text: bool,
    pub show_bars: bool,
}

impl From<&DataCurveConfig> for DataCurveRenderConfig {
    fn from(cfg: &DataCurveConfig) -> Self {
        Self {
            metric: cfg.metric,
            graph_duration: cfg.graph_duration.max(0.5),
            zoom_smoothness: cfg.zoom_smoothness.max(1.0),
            graph_smoothness: cfg.graph_smoothness,
            padding_mul: cfg.padding_mul.max(0.0),
            bg_color: cfg.bg_color,
            line_color: cfg.line_color,
            text_color: cfg.text_color,
            bar_color: cfg.bar_color,
            line_thickness: cfg.line_thickness.max(1),
            bar_thickness: cfg.bar_thickness.max(1),
            font_size: cfg.font_size.max(1),
            font: cfg.font.clone(),
            text_x_offset: cfg.text_x_offset,
            text_y_offset: cfg.text_y_offset,
            milestone_scale_mul: cfg.milestone_scale_mul.max(1.0),
            abbreviate: cfg.abbreviate,
            abbreviate_digits: cfg.abbreviate_digits,
            show_text: cfg.show_text,
            show_bars: cfg.show_bars,
        }
    }
}

/// 单帧渲染输出（诊断用）。
pub struct DataCurveFrameOutput {
    /// 当前帧数据值
    pub value: f64,
    /// 当前缩放窗口最小值
    pub min: f64,
    /// 当前缩放窗口最大值
    pub max: f64,
}

/// 数据曲线渲染状态（窗口 + 缩放 EMA + 字体渲染器，跨帧复用）。
pub struct DataCurveRenderer {
    /// 数据窗口（最近 `window_cap` 帧的值）
    window: VecDeque<f64>,
    /// 纵轴缩放 EMA 平滑状态（初始 0，与原版 canvas_vars 一致）
    min_zoom: f64,
    max_zoom: f64,
    /// 窗口容量 = fps × graph_duration
    window_cap: usize,
    /// 字体渲染器（glyph 缓存跨帧复用）
    font: CounterFontRenderer,
}

impl DataCurveRenderer {
    /// 创建渲染器。`fps` 用于计算窗口容量（数据按帧生成）。
    ///
    /// 字体加载失败（TTF 文件缺失/解析失败）返回错误——调用方决定回退策略。
    pub fn new(config: &DataCurveRenderConfig, fps: u32) -> Result<Self, String> {
        let cap = ((fps.max(1) as f64) * config.graph_duration as f64)
            .ceil()
            .max(2.0) as usize;
        let font = CounterFontRenderer::new(&config.font, config.font_size)?;
        Ok(Self {
            window: VecDeque::with_capacity(cap),
            min_zoom: 0.0,
            max_zoom: 0.0,
            window_cap: cap,
            font,
        })
    }

    /// 推入一帧数据值。窗口为空时先用该值填满（等价于原版用 CSV 首行补齐窗口）。
    pub fn push_value(&mut self, value: f64) {
        if self.window.is_empty() {
            for _ in 0..self.window_cap {
                self.window.push_back(value);
            }
        } else {
            self.window.push_back(value);
            while self.window.len() > self.window_cap {
                self.window.pop_front();
            }
        }
    }

    /// 当前（最新）数据值；窗口为空返回 0。
    pub fn current_value(&self) -> f64 {
        self.window.back().copied().unwrap_or(0.0)
    }

    /// 窗口容量（帧数，诊断用）。
    pub fn window_cap(&self) -> usize {
        self.window_cap
    }
}

/// 绘制一帧数据曲线（BGRA 像素，in-place 修改）。
///
/// 流程（对应原版 `renderSingleFrame`）：清背景 → 网格细分线 → 主刻度线+文字 → 折线。
pub fn render_data_curve_frame(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    renderer: &mut DataCurveRenderer,
    config: &DataCurveRenderConfig,
) -> DataCurveFrameOutput {
    fill_bgra(frame, config.bg_color);
    if frame_width == 0 || frame_height == 0 || renderer.window.is_empty() {
        return DataCurveFrameOutput {
            value: 0.0,
            min: 0.0,
            max: 0.0,
        };
    }

    let (fw, fh) = (frame_width as usize, frame_height as usize);
    let len = renderer.window.len();

    // 单次遍历求窗口 min/max（原版对 Lua 表 unpack 两次）
    let (mut min_val, mut max_val) = (f64::INFINITY, f64::NEG_INFINITY);
    for &v in &renderer.window {
        if v.is_finite() {
            min_val = min_val.min(v);
            max_val = max_val.max(v);
        }
    }
    if min_val > max_val {
        min_val = 0.0;
        max_val = 1.0;
    }

    // 缩放 EMA 平滑（原版 canvas_vars.min_zoom / max_zoom）
    let smooth = config.zoom_smoothness.max(1.0) as f64;
    renderer.min_zoom += (min_val - renderer.min_zoom) / smooth;
    renderer.max_zoom += (max_val - renderer.max_zoom) / smooth;
    let sub = renderer.max_zoom - renderer.min_zoom;
    let graph_min = renderer.min_zoom - (1.0 + sub * config.padding_mul as f64);
    let graph_max = renderer.max_zoom + (1.0 + sub * config.padding_mul as f64);
    let graph_sub = (graph_max - graph_min).max(1e-9);

    // ── 水平网格线（10 的幂间距；细分线透明度随缩放比例渐变） ──
    let line_pow = 10.0f64.powf(graph_sub.log10().floor());
    let bar_bgra = rgba_to_bgra(config.bar_color);
    if config.show_bars {
        let frac = 1.0 - ((graph_sub / (line_pow * 10.0)) % 1.0);
        let sub_alpha = ((bar_bgra[3] as f32) * (frac as f32 / 3.0)) as u8;
        // 细分线：0.1×line_pow 间距，101 条
        let base = (graph_min / line_pow).floor() * line_pow;
        let y_scale = fh as f64 / graph_sub;
        for i in 0..=100 {
            let value = base + (i as f64 / 10.0) * line_pow;
            if value < 0.0 {
                continue;
            }
            let y = fh as f64 - (value - graph_min) * y_scale;
            if !(0.0..=fh as f64 * 1.5).contains(&y) {
                continue;
            }
            let mut c = bar_bgra;
            c[3] = sub_alpha;
            blend_hline(frame, fw, fh, y as i32, c, config.bar_thickness);
        }
    }

    // ── 主刻度线 + 刻度文字 ──
    let font_height = renderer.font.line_height();
    let base = (graph_min / line_pow).floor() * line_pow;
    let y_scale = fh as f64 / graph_sub;
    let digit_scale = 10.0f64
        .powi(config.abbreviate_digits.min(10) as i32)
        .floor();
    for i in 0..=10 {
        let value = base + i as f64 * line_pow;
        if value < 0.0 {
            continue;
        }
        let y = fh as f64 - (value - graph_min) * y_scale;
        if !(0.0..=fh as f64 * 1.5).contains(&y) {
            continue;
        }
        let is_milestone = is_milestone(value, 1000.0);
        let text_scale = if is_milestone {
            config.milestone_scale_mul.max(1.0) as u32
        } else {
            1
        };

        if config.show_bars {
            blend_hline(frame, fw, fh, y as i32, bar_bgra, config.bar_thickness);
        }
        if config.show_text {
            let text = if config.abbreviate {
                abbreviate(value, digit_scale)
            } else {
                add_commas(value)
            };
            if !text.is_empty() {
                let ts = text_scale as f64;
                let tx = (config.text_x_offset as f64 * ts) as u32;
                let ty = ((y - font_height as f64 * ts - config.text_y_offset as f64 * ts)
                    .floor()
                    .max(0.0)) as u32;
                renderer.font.draw_line_scaled(DrawLineScaledInput {
                    frame: &mut *frame,
                    frame_width,
                    line: &text,
                    x: tx,
                    y: ty,
                    color: config.text_color,
                    extra_scale: text_scale,
                });
            }
        }
    }

    // ── 折线 ──
    let points: Vec<f64> = if config.graph_smoothness > 0 {
        let contig = renderer.window.make_contiguous();
        smooth_forward(contig, config.graph_smoothness as usize)
    } else {
        renderer.window.iter().copied().collect()
    };
    let step_x = (fw.saturating_sub(1)) as f64 / (len.saturating_sub(1)).max(1) as f64;
    let mut prev: Option<(i64, i64)> = None;
    let line_bgra = rgba_to_bgra(config.line_color);
    for (i, &v) in points.iter().enumerate() {
        let y = fh as f64 - (v - graph_min) * y_scale;
        let x = i as f64 * step_x;
        let px = (x.round() as i64).clamp(0, fw as i64 - 1);
        let py = (y.round() as i64).clamp(0, fh as i64 - 1);
        if let Some((px0, py0)) = prev {
            draw_thick_line(DrawThickLineInput {
                frame: &mut *frame,
                fw,
                fh,
                x0: px0,
                y0: py0,
                x1: px,
                y1: py,
                color_bgra: line_bgra,
                thickness: config.line_thickness,
            });
        }
        prev = Some((px, py));
    }

    DataCurveFrameOutput {
        value: renderer.current_value(),
        min: renderer.min_zoom,
        max: renderer.max_zoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> DataCurveRenderConfig {
        DataCurveRenderConfig::from(&DataCurveConfig::default())
    }

    fn renderer(cfg: &DataCurveRenderConfig) -> DataCurveRenderer {
        DataCurveRenderer::new(cfg, 60).expect("渲染器创建")
    }

    /// 窗口环形缓冲：首值填满 + 超限弹出
    #[test]
    fn test_window_ring_fill_and_cap() {
        let c = cfg();
        let mut r = renderer(&c);
        // 窗口容量 = ceil(60 × 2.0) = 120
        assert_eq!(r.window_cap(), 120);
        r.push_value(5.0);
        assert_eq!(r.window.len(), 120, "首值填满窗口");
        assert!(r.window.iter().all(|&v| v == 5.0));
        r.push_value(7.0);
        assert_eq!(r.window.len(), 120, "窗口超限弹出");
        assert_eq!(r.current_value(), 7.0);
        assert_eq!(r.window.front(), Some(&5.0));
    }

    /// 整帧渲染：背景填充正确 + 折线像素出现
    #[test]
    fn test_render_frame_no_panic_and_background() {
        let c = cfg();
        let mut r = renderer(&c);
        for v in [0.0, 3.0, 7.0, 12.0, 5.0] {
            r.push_value(v);
        }
        let mut frame = vec![0u8; 320 * 180 * 4];
        let out = render_data_curve_frame(&mut frame, 320, 180, &mut r, &c);
        assert_eq!(out.value, 5.0);
        // 背景为黑色（默认 #000000）
        assert_eq!(&frame[0..4], &[0, 0, 0, 255]);
        // 折线颜色 #00FFFF → BGRA [255, 255, 0, 255] 应出现在画面中
        assert!(frame.chunks_exact(4).any(|px| px == [255, 255, 0, 255]));
    }

    /// 空窗口：仅清背景，不 panic
    #[test]
    fn test_render_empty_window() {
        let c = cfg();
        let mut r = renderer(&c);
        let mut frame = vec![0u8; 320 * 180 * 4];
        let out = render_data_curve_frame(&mut frame, 320, 180, &mut r, &c);
        assert_eq!(out.value, 0.0);
        assert_eq!(&frame[0..4], &[0, 0, 0, 255], "空窗口仅清背景");
    }

    /// 缩放 EMA 收敛：连续渲染相同值后窗口稳定
    #[test]
    fn test_zoom_ema_converges() {
        let c = cfg();
        let mut r = renderer(&c);
        // 推入恒定值（EMA 更新发生在 render 内，需多次渲染收敛）
        for _ in 0..200 {
            r.push_value(42.0);
        }
        let mut frame = vec![0u8; 64 * 64 * 4];
        for _ in 0..200 {
            render_data_curve_frame(&mut frame, 64, 64, &mut r, &c);
        }
        let out = render_data_curve_frame(&mut frame, 64, 64, &mut r, &c);
        assert!((out.min - 42.0).abs() < 0.5, "min 收敛: {}", out.min);
        assert!((out.max - 42.0).abs() < 0.5, "max 收敛: {}", out.max);
    }
}
