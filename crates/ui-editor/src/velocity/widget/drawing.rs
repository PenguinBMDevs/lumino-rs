//! 力度/CC/Tempo Canvas 绘制函数
//!
//! 包含所有 Canvas 绘制逻辑：网格线、刻度标签、速度曲线、曲线绘制反馈等。
//!
//! # 模块结构
//!
//! 绘制函数按功能拆分到子模块：
//! - `background` — 背景绘制函数
//! - `notes` — 音符/曲线绘制函数
//! - `grid` — 网格绘制函数
//! - `labels` — 标签绘制函数

mod background;
mod grid;
mod labels;
mod notes;

pub use background::{draw_background, draw_resize_handle};
pub use grid::{draw_horizontal_lines, draw_vertical_lines};
pub use labels::draw_scale_labels;
pub use notes::{draw_curve_paint_feedback, draw_tempo_graph};

use iced_core::{Color, Point, Rectangle, Size, alignment, mouse};
use iced_widget::canvas::{self, Frame, path};

use crate::editor_state::ViewState;
use crate::grid::theme::ThemeExt;
use crate::{Renderer, Theme};

use super::super::{
    EditMode, PANEL_PADDING_X, PANEL_PADDING_Y, POINT_RADIUS, RESIZE_HANDLE_HEIGHT, VelocityPoint,
};
use super::{TempoPoint, VelocityCanvas, VelocityCanvasState};

// ── Theme-aware colors ──

/// 面板背景色
pub fn velocity_bg_color(theme: &Theme) -> Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return lumino_ui_core::theme::hc::RULER_BG;
    }
    let palette = theme.extended_palette().background;
    if theme.is_light() {
        palette.weakest.color
    } else {
        palette.base.color
    }
}

/// 面板网格线颜色
pub fn velocity_grid_line_color(theme: &Theme) -> Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return lumino_ui_core::theme::hc::GRID_LINE;
    }
    let c = theme.extended_palette().background.strongest.color;
    let alpha = if theme.is_light() { 0.10 } else { 0.08 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 面板刻度标签颜色
pub fn velocity_text_color(theme: &Theme) -> Color {
    let c = theme.text_color();
    Color::from_rgba(c.r, c.g, c.b, 0.3)
}

/// 面板顶部边框线颜色
pub fn velocity_border_color(theme: &Theme) -> Color {
    let c = theme.border_color();
    let alpha = if theme.is_light() { 0.15 } else { 0.12 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// resize 手柄背景色
pub fn velocity_handle_bg_color(theme: &Theme, hovered: bool) -> Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return if hovered {
            Color::from_rgba(1.0, 0.8, 0.0, 0.5)
        } else {
            Color::from_rgba(0.2, 0.2, 0.2, 0.3)
        };
    }
    let c = theme.extended_palette().background.strong.color;
    let alpha = if hovered { 0.5 } else { 0.25 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// resize grab 条颜色
pub fn velocity_grab_bar_color(theme: &Theme) -> Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return Color::from_rgba(1.0, 0.8, 0.0, 0.5);
    }
    let c = theme.text_color();
    let alpha = if theme.is_light() { 0.40 } else { 0.35 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 曲线绘制影响范围底色
pub fn velocity_curve_range_color(theme: &Theme) -> Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return Color::from_rgba(1.0, 0.8, 0.0, 0.12);
    }
    let c = theme.extended_palette().primary.base.color;
    let alpha = if theme.is_light() { 0.08 } else { 0.12 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 自动化节点统一蓝色，与主音轨已放置音符（MAIN_TRACK_NOTE_COLOR）
/// 视觉保持一致。Tempo 折线、Velocity 曲线反馈、CC/Bend 自动化节点均使用此色。
pub fn automation_node_color() -> Color {
    Color::from_rgb(0.2, 0.55, 1.0)
}

// ── Tempo 常量 ──

const TEMPO_BPM_MIN: f64 = 20.0;
const TEMPO_BPM_MAX: f64 = 10000.0;

/// 将 BPM 值映射到面板 Y 坐标
///
/// 采用线性映射，保证 `generate_tempo_levels` 生成的等差刻度在 Y 轴上均匀分布。
pub fn tempo_bpm_to_y(bpm: f64, bounds_height: f32) -> f32 {
    let max_y = bounds_height;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let normalized = ((bpm - TEMPO_BPM_MIN) / (TEMPO_BPM_MAX - TEMPO_BPM_MIN)) as f32;
    max_y - normalized * (max_y - min_y)
}

/// 生成 BPM 标尺刻度值
///
/// 使用等差分布，让参考线在 Y 轴上以相同间隔均匀分布。
pub fn generate_tempo_levels() -> Vec<f64> {
    let count = 9;
    let step = (TEMPO_BPM_MAX - TEMPO_BPM_MIN) / (count - 1) as f64;
    (0..count)
        .map(|i| TEMPO_BPM_MIN + step * i as f64)
        .collect()
}

/// 将弯音值 (-8192 ~ +8191) 映射到面板 Y 坐标
pub fn bend_value_to_y(value: i16, bounds_height: f32) -> f32 {
    let max_y = bounds_height;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let normalized = (value as f32 + 8192.0) / 16383.0;
    max_y - normalized * (max_y - min_y)
}

/// 计算 Tempo 控制点屏幕位置
///
/// 使用与 `tempo_bpm_to_y` 相同的线性映射，保证数据点与参考线对齐。
pub fn tempo_point_screen_pos(
    point: &TempoPoint,
    _bounds_width: f32,
    bounds_height: f32,
    view: &ViewState,
    _min_bpm: f64,
    _bpm_range: f64,
) -> Point {
    let point_x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
    let point_y = tempo_bpm_to_y(point.bpm, bounds_height);
    Point::new(point_x, point_y)
}

// ── 曲线绘制反馈的画布参数 ──

/// 曲线绘制反馈的画布参数
pub struct CurvePaintCanvasParams {
    size: Size,
    view: ViewState,
    bounds: Rectangle,
}
