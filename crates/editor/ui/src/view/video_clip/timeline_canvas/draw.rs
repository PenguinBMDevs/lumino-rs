//! 剪辑带时间轴 Canvas 绘制与几何
//!
//! 从 `timeline_canvas.rs` 拆出（保持单文件 <400 行约束）：
//! 标尺、视频/音频轨道条（圆角 + 首尾裁剪把手）、走带指示线的全部
//! 绘制逻辑，以及轨道条命中测试纯函数。

use iced_core::{Color, Point, Rectangle, Size, border};
use iced_widget::canvas::{Frame, Path, Text};

use lumino_message::video_clip::ClipTrack;
use lumino_ui_core::state::video_clip_state::ClipTrackEdit;

use crate::view::video_clip::timeline_canvas::{
    MAJOR_INTERVAL, MINOR_INTERVAL, PIXELS_PER_SEC, RULER_HEIGHT, TRACK_HEIGHT, TRACK_SPACING,
};
use crate::{Renderer, Theme};

/// 轨道条圆角半径（像素）
pub const TRACK_CORNER_RADIUS: f32 = 6.0;

/// 首尾裁剪把手绘制宽度（像素，视觉保持纤细）
pub const HANDLE_WIDTH: f32 = 8.0;
/// 首尾裁剪把手命中宽度（像素，远大于绘制宽度以放宽抓取容差）。
///
/// 起始把手贴在轨道最左缘、且外层 container 带 8px padding，
/// 8px 绘制宽度极易被误点为 padding 或误抓条身（Body→整体平移而非裁剪，
/// 用户感知为"拖头部改不了长度"）。命中区放宽到 16px 让"拖头部改长度"可靠生效。
pub const HANDLE_HIT_WIDTH: f32 = 16.0;

/// 轨道条屏幕几何（已含滚动偏移，x 可为负表示部分滚出视口）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackGeom {
    /// 屏幕左缘 x
    pub x: f32,
    /// 屏幕顶缘 y
    pub y: f32,
    /// 屏幕宽度（≥0）
    pub w: f32,
}

impl TrackGeom {
    /// 由素材编辑状态计算轨道条屏幕几何。
    ///
    /// * `source_len` — 素材源长（秒，即曲目时长）
    /// * `pps_zoom` — 每秒像素 × 缩放
    pub fn new(
        edit: &ClipTrackEdit,
        source_len: f32,
        pps_zoom: f32,
        scroll_x: f32,
        y: f32,
    ) -> Self {
        Self {
            x: edit.visible_start() * pps_zoom - scroll_x,
            w: (edit.visible_len(source_len) * pps_zoom).max(0.0),
            y,
        }
    }

    /// 该轨道的 y 坐标（视频/音频双轨布局，与绘制侧一致）
    pub fn track_y(track: ClipTrack) -> f32 {
        match track {
            ClipTrack::Video => RULER_HEIGHT + TRACK_SPACING,
            ClipTrack::Audio => RULER_HEIGHT + TRACK_SPACING + TRACK_HEIGHT + TRACK_SPACING,
        }
    }
}

/// 轨道条命中区域
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitZone {
    /// 首端裁剪把手
    HandleStart,
    /// 尾端裁剪把手
    HandleEnd,
    /// 条身（整体移动）
    Body,
}

/// 命中测试：屏幕点落在哪条轨道的哪个区域。
///
/// 把手优先于条身；未命中任何轨道条带返回 `(None, None)`。
pub fn hit_test_track(
    video_geom: TrackGeom,
    audio_geom: TrackGeom,
    p: Point,
) -> (Option<ClipTrack>, Option<HitZone>) {
    for (track, g) in [
        (ClipTrack::Video, video_geom),
        (ClipTrack::Audio, audio_geom),
    ] {
        if p.y < g.y || p.y > g.y + TRACK_HEIGHT || g.w <= 0.0 {
            continue;
        }
        let local_x = p.x - g.x;
        if !(0.0..=g.w).contains(&local_x) {
            continue;
        }
        let zone = if local_x <= HANDLE_HIT_WIDTH {
            HitZone::HandleStart
        } else if local_x >= g.w - HANDLE_HIT_WIDTH {
            HitZone::HandleEnd
        } else {
            HitZone::Body
        };
        return (Some(track), Some(zone));
    }
    (None, None)
}

/// 绘制剪辑带时间轴全部内容（标尺 / 圆角双轨 + 裁剪把手 / 走带线）
pub(super) fn draw_timeline(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    bounds: Rectangle,
    c: &super::TimelineCanvas,
) {
    let palette = theme.extended_palette();
    let weak_color = palette.background.weak.color;
    let strong_color = palette.background.strong.color;
    let weak_text = palette.background.weak.text;

    // ── 背景 ──
    frame.fill_rectangle(
        Point::ORIGIN,
        bounds.size(),
        palette.background.weakest.color,
    );

    // ── 标尺：可视窗口内的秒刻度 ──
    let pps_zoom = PIXELS_PER_SEC * c.zoom;
    let start_sec = (c.scroll_x / pps_zoom).floor().max(0.0);
    let end_sec = ((c.scroll_x + bounds.width) / pps_zoom).min(c.duration_secs);
    let mut t = start_sec;
    while t <= end_sec {
        let is_major = (t % MAJOR_INTERVAL).abs() < 0.01;
        let tick_h = if is_major { 12.0 } else { 6.0 };
        let x = t * pps_zoom - c.scroll_x;
        if x >= -1.0 && x <= bounds.width + 1.0 {
            let color = if is_major { strong_color } else { weak_color };
            frame.fill_rectangle(
                Point::new(x, RULER_HEIGHT - tick_h),
                Size::new(1.0, tick_h),
                color,
            );
            if is_major {
                frame.fill_text(Text {
                    content: format!("{t:.0}s"),
                    position: Point::new(x + 3.0, 2.0),
                    size: 10.0.into(),
                    color: weak_text,
                    ..Text::default()
                });
            }
        }
        t += MINOR_INTERVAL;
    }

    // ── 视频轨（蓝）与音频轨（绿）：圆角矩形 + 首尾裁剪把手 ──
    draw_track(
        frame,
        TrackGeom::new(
            &c.video_edit,
            c.duration_secs,
            pps_zoom,
            c.scroll_x,
            TrackGeom::track_y(ClipTrack::Video),
        ),
        Color::from_rgb(0.2, 0.6, 0.95),
        "视频",
        bounds.width,
    );
    draw_track(
        frame,
        TrackGeom::new(
            &c.audio_edit,
            c.duration_secs,
            pps_zoom,
            c.scroll_x,
            TrackGeom::track_y(ClipTrack::Audio),
        ),
        Color::from_rgb(0.25, 0.75, 0.35),
        "音频",
        bounds.width,
    );

    // ── 走带指示线：画在播放位置对应的屏幕坐标，纵贯全高 ──
    if let Some(playhead_x) = c.playhead_screen_x(bounds.width) {
        frame.fill_rectangle(
            Point::new(playhead_x, 0.0),
            Size::new(2.0, bounds.height),
            Color::from_rgb(0.95, 0.25, 0.25),
        );
    }
}

/// 绘制单条轨道条：圆角矩形主体 + 两端半透明裁剪把手 + 标签
fn draw_track(
    frame: &mut Frame<Renderer>,
    geom: TrackGeom,
    base_color: Color,
    label: &str,
    viewport_w: f32,
) {
    if geom.w <= 0.0 || geom.x > viewport_w || geom.x + geom.w < 0.0 {
        return; // 完全滚出视口或无可视长度
    }

    // 圆角主体：Path::rounded_rectangle + Frame::fill（fill_rectangle 无圆角能力）
    let body = Path::rounded_rectangle(
        Point::new(geom.x, geom.y),
        Size::new(geom.w, TRACK_HEIGHT),
        border::Radius::from(TRACK_CORNER_RADIUS),
    );
    frame.fill(&body, base_color);

    // 首尾裁剪把手：两端内侧半透明白窄条（提示可拖拽裁剪）
    for hx in [geom.x, geom.x + geom.w - HANDLE_WIDTH] {
        let handle = Path::rounded_rectangle(
            Point::new(hx, geom.y + 4.0),
            Size::new(HANDLE_WIDTH, TRACK_HEIGHT - 8.0),
            border::Radius::from(2.0),
        );
        frame.fill(&handle, Color::from_rgba(1.0, 1.0, 1.0, 0.45));
    }

    // 轨道标签（跟随内容滚动）
    let label_x = geom.x + 10.0;
    if label_x > -60.0 && label_x < viewport_w {
        frame.fill_text(Text {
            content: label.to_string(),
            position: Point::new(label_x, geom.y + (TRACK_HEIGHT - 14.0) / 2.0),
            size: 11.0.into(),
            color: Color::WHITE,
            ..Text::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::video_clip::timeline_canvas::{RULER_HEIGHT, TRACK_HEIGHT, TRACK_SPACING};
    use lumino_ui_core::state::video_clip_state::ClipTrackEdit;

    #[test]
    fn test_track_geom_visible_window() {
        let mut e = ClipTrackEdit::default();
        e.set_offset(2.0);
        e.set_trim_start(1.0, 10.0);
        e.set_trim_end(0.5, 10.0);
        // pps_zoom=80、无滚动：x = 3×80=240，宽 = 8.5×80=680
        let g = TrackGeom::new(&e, 10.0, 80.0, 0.0, 40.0);
        assert!((g.x - 240.0).abs() < f32::EPSILON);
        assert!((g.w - 680.0).abs() < f32::EPSILON);

        // 滚动偏移直接平移 x
        let g2 = TrackGeom::new(&e, 10.0, 80.0, 100.0, 40.0);
        assert!((g2.x - 140.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_hit_track_zones_and_priority() {
        // 视频轨 [100, 300]、音频轨 y 更靠下
        let video = TrackGeom {
            x: 100.0,
            y: RULER_HEIGHT + TRACK_SPACING,
            w: 200.0,
        };
        let audio = TrackGeom {
            y: video.y + TRACK_HEIGHT + TRACK_SPACING,
            ..video
        };

        // 首端把手（左缘 8px 内）优先于条身
        let (t, z) = hit_test_track(video, audio, Point::new(104.0, video.y + 10.0));
        assert_eq!(t, Some(ClipTrack::Video));
        assert_eq!(z, Some(HitZone::HandleStart));

        // 尾端把手
        let (_, z) = hit_test_track(video, audio, Point::new(295.0, video.y + 10.0));
        assert_eq!(z, Some(HitZone::HandleEnd));

        // 条身中部 → Body
        let (t, z) = hit_test_track(video, audio, Point::new(200.0, video.y + 10.0));
        assert_eq!(t, Some(ClipTrack::Video));
        assert_eq!(z, Some(HitZone::Body));

        // 音频轨命中
        let (t, _) = hit_test_track(video, audio, Point::new(200.0, audio.y + 10.0));
        assert_eq!(t, Some(ClipTrack::Audio));

        // 完全未命中（轨道带外 / 素材外）
        let (t, z) = hit_test_track(video, audio, Point::new(50.0, video.y + 10.0));
        assert_eq!(t, None);
        assert_eq!(z, None);
        let (t, _) = hit_test_track(
            video,
            audio,
            Point::new(200.0, video.y + TRACK_HEIGHT + 1.0),
        );
        assert_eq!(t, None);
    }
}
