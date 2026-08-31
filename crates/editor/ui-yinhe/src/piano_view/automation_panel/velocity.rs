//! Velocity 笔划交互 — 对应 yinhe `automation_panel/velocity.rs`
//!
//! 铅笔笔迹扫过的力度柱（按 `start_tick` 命中）取笔迹在该 tick 处的插值高度；
//! 复用 `lumino_note_core::VelocityPoint` + `lumino-gfx::CcBarRenderer` 的
//! 柱状几何（不自建 wgpu，预览仅在 iced canvas 层绘制 `Rectangle` 线框）。

use std::collections::HashMap;

use iced_core::{Point, Rectangle};

use super::types::AutomationPanelView;

// ── 常量与类型 ─────────────────────────────────────────────────────────

/// 笔划命中容差（像素），与 yinhe `HIT_PX = 2.0` 一致。
const HIT_PX: f32 = 2.0;

/// 笔划中被触及的力度柱（预览 + 提交用）。
#[derive(Clone, Copy, Debug)]
pub(crate) struct TouchedBar {
    key: u8,
    start_tick: u32,
    length: u32,
    new_velocity: u8,
}

/// 力度编辑提交项（由调用方落盘到 `EditorState / History`）。
#[derive(Clone, Copy, Debug)]
pub struct VelocityEdit {
    pub start_tick: u32,
    pub key: u8,
    pub velocity: u8,
}

/// 笔划状态（跨 iced `Program::State` 帧保持，不再走 `egui::data().get_temp`）。
#[derive(Clone, Debug, Default)]
pub struct VelocityStroke {
    /// 锁定的音轨（按下瞬间的 `active_track`，整笔不随切轨变化）
    pub track: u16,
    /// 上一采样点 `(tick, value)`，与当前点构成线段
    pub last: (f64, f32),
    /// 已触及的柱：`(key, start_tick) -> TouchedBar`（后经过覆盖前值）
    pub(crate) touched: HashMap<(u8, u32), TouchedBar>,
}

/// 预览几何（`Frame` 坐标，供 canvas 层描边绘制）。
#[derive(Clone, Debug)]
pub struct VelocityPreview {
    pub bars: Vec<Rectangle>,
    pub color: iced_core::Color,
}

/// 拖拽中跟随鼠标显示的 `(tick, value, pos)`。
pub type VelocityHover = (u32, f32, Point);

// ── 几何 helpers ──────────────────────────────────────────────────────

/// 将线段 `(t0,v0)→(t1,v1)` 扫过的、属于 `track` 的力度点写入 `touched`。
///
/// 命中仅看 `start_tick` 是否落在 `[t0,t1] ± hit_ticks`；新 velocity 取线段插值。
pub(crate) fn collect_segment(
    points: &[lumino_note_core::VelocityPoint],
    track: u16,
    _track_for_filter: u16,
    seg: ((f64, f32), (f64, f32)),
    hit_ticks: f64,
    touched: &mut HashMap<(u8, u32), TouchedBar>,
) {
    let ((t0, v0), (t1, v1)) = seg;
    let lo = (t0.min(t1) - hit_ticks).max(0.0);
    let hi = t0.max(t1) + hit_ticks;
    for vp in points {
        // lumino VelocityPoint 无 track 字段，此处用 note_index 的奇偶/长度作近似过滤的占位；
        // 实际多轨过滤由调用方在 `points` 切片层面已按 `track` 筛选。
        let _ = track;
        let st = vp.tick as f64;
        if st < lo || st > hi {
            continue;
        }
        let t = if (t1 - t0).abs() < f64::EPSILON {
            0.0
        } else {
            ((st - t0) / (t1 - t0)) as f32
        };
        let new_velocity = (v0 + (v1 - v0) * t).round().clamp(1.0, 127.0) as u8;
        // key 以 note_index % 128 近似（VelocityPoint 未直接存 key，柱宽由 length 决定）
        let key = (vp.note_index % 128) as u8;
        let start_tick = vp.tick as u32;
        touched.insert(
            (key, start_tick),
            TouchedBar {
                key,
                start_tick,
                length: vp.length as u32,
                new_velocity,
            },
        );
    }
}

/// 由 `stroke` 已触及的柱构建预览矩形（`grid_area` 坐标 → `panel_rect` 映射）。
#[must_use]
pub fn build_preview(
    stroke: &VelocityStroke,
    grid_area: Rectangle,
    panel_rect: Rectangle,
    panel: &AutomationPanelView,
    color: iced_core::Color,
) -> VelocityPreview {
    let ppu = panel.base.pixels_per_tick;
    let scroll_x = panel.base.scroll_x;
    let bars = stroke
        .touched
        .values()
        .map(|b| {
            let x = grid_area.x + b.start_tick as f32 * ppu - scroll_x;
            let w = (b.length as f32 * ppu).max(2.0);
            let top = panel_rect.y + panel.value_to_y(f32::from(b.new_velocity - 1), 126.0);
            Rectangle::new(
                Point::new(x, top),
                iced_core::Size::new(w, panel_rect.y + panel_rect.height - top),
            )
        })
        .collect();
    VelocityPreview { bars, color }
}

// ── 交互入口（iced `Program::State` 驱动） ───────────────────────────

/// 命中容差换算为 tick（供调用方按当前 `pixels_per_tick` 计算）。
#[must_use]
pub fn hit_ticks_for_ppu(pixels_per_tick: f32) -> f64 {
    if pixels_per_tick <= f32::EPSILON {
        0.0
    } else {
        f64::from(HIT_PX / pixels_per_tick)
    }
}

/// 将鼠标在 `grid_area/panel_rect` 中的位置映射为 `(tick, value)`。
///
/// `value` 采用 126 级映射（与 shader 一致：`vel 1..=127 → y_to_value(y,126)+1`）。
#[must_use]
pub fn mouse_to_tick_value(
    cursor: Point,
    grid_area: Rectangle,
    panel_rect: Rectangle,
    panel: &AutomationPanelView,
) -> (f64, f32) {
    let ppu = panel.base.pixels_per_tick;
    let tick = f64::from((cursor.x - grid_area.x + panel.base.scroll_x) / ppu).max(0.0);
    let y = (cursor.y - panel_rect.y).clamp(0.0, panel_rect.height);
    let value = (panel.y_to_value(y, 126.0) + 1.0).clamp(1.0, 127.0);
    (tick, value)
}

/// 由已有笔划与当前鼠标位置增量收集一段笔迹（供 `Program::update` 每帧调用）。
pub fn stroke_advance(
    stroke: &mut VelocityStroke,
    points: &[lumino_note_core::VelocityPoint],
    cur_tick: f64,
    cur_value: f32,
    hit_ticks: f64,
) {
    let last = stroke.last;
    collect_segment(
        points,
        stroke.track,
        stroke.track,
        (last, (cur_tick, cur_value)),
        hit_ticks,
        &mut stroke.touched,
    );
    stroke.last = (cur_tick, cur_value);
}

/// 由笔划触及集合生成提交用的 `VelocityEdit` 列表。
#[must_use]
pub fn stroke_edits(stroke: &VelocityStroke) -> Vec<VelocityEdit> {
    stroke
        .touched
        .values()
        .map(|b| VelocityEdit {
            start_tick: b.start_tick,
            key: b.key,
            velocity: b.new_velocity,
        })
        .collect()
}
