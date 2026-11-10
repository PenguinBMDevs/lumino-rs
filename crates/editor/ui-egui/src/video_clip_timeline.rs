//! 视频剪辑时间轴的 egui 实现（标尺 + 双轨条 + 走带线 + 命中测试）。
//!
//! 对应 iced `view/video_clip/timeline_canvas{,/draw}`：几何常量、刻度规则、
//! 圆角轨道条与首尾裁剪把手、命中区划分全部同口径。交互（scrub / 移动 / 裁剪）
//! 由调用方按 [`HitZone`] 分派，本模块只负责「画什么」与「点到哪」。
//!
//! 与 iced 的差异：不依赖 `iced_widget::canvas`，只用 egui painter。

use egui::{Align2, CornerRadius, FontId, Rect, Stroke};

use crate::theme;

/// 每秒基准像素密度（zoom = 1）。
pub const PIXELS_PER_SEC: f32 = 80.0;
/// 标尺区高度。
pub const RULER_HEIGHT: f32 = 24.0;
/// 轨道条高度。
pub const TRACK_HEIGHT: f32 = 48.0;
/// 轨道条间距。
pub const TRACK_SPACING: f32 = 8.0;
/// 主刻度间隔（秒）。
pub const MAJOR_INTERVAL: f32 = 5.0;
/// 次刻度间隔（秒）。
pub const MINOR_INTERVAL: f32 = 1.0;
/// 轨道条圆角半径。
pub const TRACK_CORNER_RADIUS: f32 = 6.0;
/// 把手视觉宽度。
pub const HANDLE_WIDTH: f32 = 8.0;
/// 把手命中宽度（比视觉宽，便于抓取）。
pub const HANDLE_HIT_WIDTH: f32 = 16.0;

/// 轨道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipTrack {
    /// 视频轨。
    Video,
    /// 音频轨。
    Audio,
}

/// 轨道素材的可见区间（秒，已含偏移与裁剪）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClipSpan {
    /// 可见起点（秒）。
    pub start: f32,
    /// 可见长度（秒）。
    pub len: f32,
}

/// 时间轴数据（每帧由生产侧装配）。
#[derive(Debug, Clone, PartialEq)]
pub struct VideoTimelineModel {
    /// 素材源长（秒）。
    pub duration_secs: f32,
    /// 水平缩放倍率。
    pub zoom: f32,
    /// 水平滚动（像素）。
    pub scroll_x: f32,
    /// 播放位置（秒）。
    pub playhead_secs: f32,
    /// 视频轨可见区间。
    pub video: ClipSpan,
    /// 音频轨可见区间。
    pub audio: ClipSpan,
}

impl Default for VideoTimelineModel {
    fn default() -> Self {
        Self {
            duration_secs: 0.0,
            zoom: 1.0,
            scroll_x: 0.0,
            playhead_secs: 0.0,
            video: ClipSpan::default(),
            audio: ClipSpan::default(),
        }
    }
}

/// 轨道条的屏幕几何。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackGeom {
    /// 左缘。
    pub x: f32,
    /// 上缘。
    pub y: f32,
    /// 宽度（≥ 0）。
    pub w: f32,
}

impl TrackGeom {
    /// 由可见区间算屏幕几何（局部坐标，相对时间轴控件）。
    pub fn new(span: ClipSpan, pps_zoom: f32, scroll_x: f32, y: f32) -> Self {
        Self {
            x: span.start * pps_zoom - scroll_x,
            w: (span.len * pps_zoom).max(0.0),
            y,
        }
    }

    /// 轨道条的固定 y（与 iced 布局一致）。
    pub fn track_y(track: ClipTrack) -> f32 {
        match track {
            ClipTrack::Video => RULER_HEIGHT + TRACK_SPACING,
            ClipTrack::Audio => RULER_HEIGHT + TRACK_SPACING + TRACK_HEIGHT + TRACK_SPACING,
        }
    }
}

/// 命中区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitZone {
    /// 标尺（点击/拖拽定位）。
    Ruler,
    /// 轨道条整体（平移）。
    Body(ClipTrack),
    /// 起始把手（裁剪头部）。
    Head(ClipTrack),
    /// 结束把手（裁剪尾部）。
    Tail(ClipTrack),
}

/// 命中测试（`local` 为时间轴控件局部坐标）。
pub fn hit_test(local: egui::Pos2, model: &VideoTimelineModel, width: f32) -> Option<HitZone> {
    let _ = width;
    if local.y < RULER_HEIGHT {
        return Some(HitZone::Ruler);
    }
    let pps = PIXELS_PER_SEC * model.zoom;
    for (track, span) in [
        (ClipTrack::Video, model.video),
        (ClipTrack::Audio, model.audio),
    ] {
        let geom = TrackGeom::new(span, pps, model.scroll_x, TrackGeom::track_y(track));
        let bar = Rect::from_min_size(egui::pos2(geom.x, geom.y), egui::vec2(geom.w, TRACK_HEIGHT));
        if !bar.contains(local) {
            continue;
        }
        if local.x <= bar.left() + HANDLE_HIT_WIDTH {
            return Some(HitZone::Head(track));
        }
        if local.x >= bar.right() - HANDLE_HIT_WIDTH {
            return Some(HitZone::Tail(track));
        }
        return Some(HitZone::Body(track));
    }
    None
}

/// 主/次刻度（秒）。
pub fn ticks(model: &VideoTimelineModel, width: f32) -> Vec<(f32, bool)> {
    let pps = PIXELS_PER_SEC * model.zoom;
    if pps <= 0.0 {
        return Vec::new();
    }
    let start = model.scroll_x.max(0.0) / pps;
    let end = (model.scroll_x + width) / pps;
    let mut out = Vec::new();
    let mut t = (start / MINOR_INTERVAL).floor() * MINOR_INTERVAL;
    while t <= end.max(0.0) + MINOR_INTERVAL {
        let major = (t / MAJOR_INTERVAL).fract().abs() < 1e-3;
        if t >= 0.0 {
            out.push((t, major));
        }
        t += MINOR_INTERVAL;
    }
    out
}

/// 绘制时间轴（`rect` = 控件区域）。
pub fn show(ui: &egui::Ui, rect: Rect, model: &VideoTimelineModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 0, theme::base_bg());

    let pps = PIXELS_PER_SEC * model.zoom;
    // 标尺底 + 刻度。
    painter.rect_filled(
        Rect::from_min_size(rect.min, egui::vec2(rect.width(), RULER_HEIGHT)),
        0,
        theme::roll_ruler_bg(),
    );
    for (t, major) in ticks(model, rect.width()) {
        let x = rect.left() + t * pps - model.scroll_x;
        if x < rect.left() - 1.0 || x > rect.right() {
            continue;
        }
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(x, if major { 6.0 } else { 14.0 }),
                egui::vec2(1.0, RULER_HEIGHT - if major { 6.0 } else { 14.0 }),
            ),
            0,
            theme::text_muted(),
        );
        if major {
            painter.text(
                egui::pos2(x + 3.0, 2.0),
                Align2::LEFT_TOP,
                format!("{}s", t as i64),
                FontId::proportional(10.0),
                theme::text(),
            );
        }
    }

    // 双轨条 + 把手。
    for (track, span) in [
        (ClipTrack::Video, model.video),
        (ClipTrack::Audio, model.audio),
    ] {
        let geom = TrackGeom::new(span, pps, model.scroll_x, TrackGeom::track_y(track));
        if geom.w <= 0.0 {
            continue;
        }
        let bar = Rect::from_min_size(
            egui::pos2(rect.left() + geom.x, rect.top() + geom.y),
            egui::vec2(geom.w, TRACK_HEIGHT),
        );
        if !bar.intersects(rect) {
            continue;
        }
        painter.rect(
            bar,
            CornerRadius::same(TRACK_CORNER_RADIUS as u8),
            theme::card_bg(),
            Stroke::new(1.0, theme::border()),
            egui::StrokeKind::Inside,
        );
        for handle in [
            Rect::from_min_max(bar.min, egui::pos2(bar.left() + HANDLE_WIDTH, bar.bottom())),
            Rect::from_min_max(egui::pos2(bar.right() - HANDLE_WIDTH, bar.top()), bar.max),
        ] {
            painter.rect_filled(handle, CornerRadius::same(2), theme::accent());
        }
    }

    // 走带线。
    let x = rect.left() + model.playhead_secs * pps - model.scroll_x;
    if x >= rect.left() && x <= rect.right() {
        painter.rect_filled(
            Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(1.0, rect.height())),
            0,
            theme::playhead(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> VideoTimelineModel {
        VideoTimelineModel {
            duration_secs: 60.0,
            zoom: 1.0,
            scroll_x: 0.0,
            playhead_secs: 0.0,
            video: ClipSpan {
                start: 0.0,
                len: 30.0,
            },
            audio: ClipSpan {
                start: 0.0,
                len: 30.0,
            },
        }
    }

    #[test]
    fn ticks_follow_major_and_minor_intervals() {
        let ticks = ticks(&model(), 500.0);
        let majors: Vec<f32> = ticks.iter().filter(|(_, m)| *m).map(|(t, _)| *t).collect();
        assert!(
            majors.contains(&0.0) && majors.contains(&5.0),
            "主刻度每 5 秒"
        );
        let minors = ticks.len() - majors.len();
        assert!(minors >= 2, "次刻度每 1 秒");
    }

    #[test]
    fn hit_test_splits_ruler_body_and_handles() {
        let m = model();
        // 标尺：y < 24。
        assert_eq!(
            hit_test(egui::pos2(100.0, 10.0), &m, 400.0),
            Some(HitZone::Ruler)
        );
        // 视频条：y = 24+8 .. +48，条宽 30s*80 = 2400（宽于视口）。
        let y = TrackGeom::track_y(ClipTrack::Video) + TRACK_HEIGHT / 2.0;
        assert_eq!(
            hit_test(egui::pos2(300.0, y), &m, 400.0),
            Some(HitZone::Body(ClipTrack::Video))
        );
        // 起始把手：x <= 16。
        assert_eq!(
            hit_test(egui::pos2(8.0, y), &m, 400.0),
            Some(HitZone::Head(ClipTrack::Video))
        );
        // 音频条。
        let ya = TrackGeom::track_y(ClipTrack::Audio) + TRACK_HEIGHT / 2.0;
        assert_eq!(
            hit_test(egui::pos2(300.0, ya), &m, 400.0),
            Some(HitZone::Body(ClipTrack::Audio))
        );
        // 空白处。
        assert_eq!(hit_test(egui::pos2(300.0, 300.0), &m, 400.0), None);
    }

    #[test]
    fn track_geometry_uses_zoom_and_scroll() {
        let g = TrackGeom::new(
            ClipSpan {
                start: 10.0,
                len: 5.0,
            },
            160.0,
            100.0,
            0.0,
        );
        assert!((g.x - (10.0 * 160.0 - 100.0)).abs() < 0.01);
        assert!((g.w - 5.0 * 160.0).abs() < 0.01);
    }
}
