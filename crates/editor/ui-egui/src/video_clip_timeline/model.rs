//! 时间轴数据、几何与命中测试：常量、轨道、编辑量、屏幕几何、刻度。
//!
//! 本片只做纯计算：不碰 painter，也不读输入。绘制见 `paint`，交互见 `input`。

use egui::Rect;

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

/// 轨道素材的**原始编辑量**（秒）：
/// 可见区间 = `[offset + trim_in, offset + duration - trim_out]`。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClipEdit {
    /// 整体偏移。
    pub offset: f32,
    /// 首端裁剪。
    pub trim_in: f32,
    /// 尾端裁剪。
    pub trim_out: f32,
}

impl ClipEdit {
    /// 可见起点。
    pub fn visible_start(&self) -> f32 {
        self.offset + self.trim_in
    }

    /// 可见长度（源长 `duration`）。
    pub fn visible_len(&self, duration: f32) -> f32 {
        (duration - self.trim_in - self.trim_out).max(0.0)
    }

    /// 可见区间 `(start, len)`。
    pub fn span(&self, duration: f32) -> (f32, f32) {
        (self.visible_start(), self.visible_len(duration))
    }
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
    /// 视频轨原始编辑量。
    pub video: ClipEdit,
    /// 音频轨原始编辑量。
    pub audio: ClipEdit,
    /// 时间轴可视宽度（像素，滚动/缩放消息需要）。
    pub viewport_w: f32,
    /// 标题行矩形（探针实测）；`None` = 未布局，不画标题行。
    pub title_row: Option<Rect>,
    /// 水平滚动条矩形（探针实测）；`None` = 未布局。
    pub scrollbar: Option<Rect>,
    /// 内容像素宽（`content_secs × PIXELS_PER_SEC × zoom`）。
    pub content_w: f32,
    /// 标题文案（「剪辑带」）。
    pub title: String,
    /// 信息行文案（时长 / 位置 / 缩放 / 快捷键提示）。
    pub info: String,
    /// 是否正在播放（决定播放/暂停图标）。
    pub playing: bool,
    /// 倒带按钮的悬停说明。
    pub rewind_tooltip: String,
    /// 播放按钮的悬停说明。
    pub play_tooltip: String,
    /// 暂停按钮的悬停说明。
    pub pause_tooltip: String,
}

impl VideoTimelineModel {
    /// 内容秒长：素材源长与两条轨道右缘取大（越界部分可滚回）。
    pub fn content_secs(&self) -> f32 {
        let (vs, vl) = self.video.span(self.duration_secs);
        let (as_, al) = self.audio.span(self.duration_secs);
        self.duration_secs.max(vs + vl).max(as_ + al)
    }
}

impl Default for VideoTimelineModel {
    fn default() -> Self {
        Self {
            duration_secs: 0.0,
            zoom: 1.0,
            scroll_x: 0.0,
            playhead_secs: 0.0,
            video: ClipEdit::default(),
            audio: ClipEdit::default(),
            viewport_w: 1.0,
            title_row: None,
            scrollbar: None,
            content_w: 0.0,
            title: String::new(),
            info: String::new(),
            playing: false,
            rewind_tooltip: String::new(),
            play_tooltip: String::new(),
            pause_tooltip: String::new(),
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
    pub fn new(span: (f32, f32), pps_zoom: f32, scroll_x: f32, y: f32) -> Self {
        Self {
            x: span.0 * pps_zoom - scroll_x,
            w: (span.1 * pps_zoom).max(0.0),
            y,
        }
    }

    /// 轨道条的固定 y（与 上一代 布局一致）。
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
    for (track, edit) in [
        (ClipTrack::Video, model.video),
        (ClipTrack::Audio, model.audio),
    ] {
        let geom = TrackGeom::new(
            edit.span(model.duration_secs),
            pps,
            model.scroll_x,
            TrackGeom::track_y(track),
        );
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
