//! 视频剪辑时间轴的 egui 实现（标尺 + 双轨条 + 走带线 + 命中测试）。
//!
//! 对应 iced `view/video_clip/timeline_canvas{,/draw}`：几何常量、刻度规则、
//! 圆角轨道条与首尾裁剪把手、命中区划分全部同口径。交互（scrub / 移动 / 裁剪）
//! 由调用方按 [`HitZone`] 分派，本模块只负责「画什么」与「点到哪」。
//!
//! 与 iced 的差异：不依赖 `iced_widget::canvas`，只用 egui painter。

use egui::{Align2, CornerRadius, FontId, Rect, Vec2};

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

/// 绘制时间轴（`rect` = 控件区域）。
///
/// 颜色/几何逐项对齐 iced `timeline_canvas/draw.rs`：
/// 背景 `weakest`、标尺主刻度 `strong` 12px / 次刻度 `weak` 6px（贴底）、
/// 主刻度标签 10px `weak.text`、视频条 `rgb(0.2,0.6,0.95)`、音频条
/// `rgb(0.25,0.75,0.35)`、把手为 45% 白（8×40，y+4）、标签白色 11px（x+10 居中）、
/// 走带线 `rgb(0.95,0.25,0.25)` 宽 2。
/// 标题行：标题 + 倒带/播放按钮 + 信息文本（全部 egui 自绘，左侧对齐）。
///
/// 与 iced 原布局同口径：行内左边距 8、间距 4、按钮 28×20（图标 16 + 内边距 6/2）、
/// 标题 12px、信息 11px。行矩形由 iced 的探测占位提供，跨框架不存在文本度量对齐。
pub fn show_title_row(
    ui: &egui::Ui,
    model: &VideoTimelineModel,
    icons: &mut crate::icons::IconCache,
) {
    let Some(rect) = model.title_row else {
        return;
    };
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    let cy = rect.center().y;
    let mut x = rect.left() + 8.0;

    // 标题（12px，正文色）。
    painter.text(
        egui::pos2(x, cy),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(12.0),
        theme::text(),
    );
    x += text_width(ui, &model.title, 12.0) + 4.0;

    let btn = Vec2::new(BUTTON_W, BUTTON_H);
    let rewind = Rect::from_min_size(egui::pos2(x, cy - BUTTON_H / 2.0), btn);
    x += BUTTON_W + 4.0;
    let play = Rect::from_min_size(egui::pos2(x, cy - BUTTON_H / 2.0), btn);
    x += BUTTON_W + 4.0;

    let rewind_resp = ui.interact(
        rewind,
        egui::Id::unique("lumino.timeline.rewind"),
        egui::Sense::click(),
    );
    let play_resp = ui.interact(
        play,
        egui::Id::unique("lumino.timeline.play"),
        egui::Sense::click(),
    );
    for (resp, rect) in [(&rewind_resp, rewind), (&play_resp, play)] {
        if resp.hovered() {
            painter.rect_filled(rect, 4, theme::toolbar_bg());
        }
    }
    if rewind_resp.clicked() {
        crate::chrome::push_video_timeline_action(ui.ctx(), VideoTimelineAction::Rewound);
    }
    if play_resp.clicked() {
        crate::chrome::push_video_timeline_action(ui.ctx(), VideoTimelineAction::PlayToggled);
    }
    let icon_size = 16.0;
    let rewind_icon = Rect::from_center_size(rewind.center(), Vec2::splat(icon_size));
    icons.paint(&painter, crate::icons::Icon::SkipBackward, rewind_icon);
    let play_icon = if model.playing {
        crate::icons::Icon::Pause
    } else {
        crate::icons::Icon::Play
    };
    let play_icon_rect = Rect::from_center_size(play.center(), Vec2::splat(icon_size));
    icons.paint(&painter, play_icon, play_icon_rect);

    // 信息文本（11px，次级色）。
    painter.text(
        egui::pos2(x, cy),
        Align2::LEFT_CENTER,
        &model.info,
        FontId::proportional(11.0),
        theme::text_muted(),
    );

    // 标题行由 egui 独占：iced 的同位占位行不再需要事件。
    crate::chrome::menu::extend_blocked(ui.ctx(), [rect]);
}

/// 单行文本宽度（与绘制同源：同一字号与字体族）。
fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::text())
        .size()
        .x
}

/// 传输按钮尺寸（图标 16 + 内边距 6/2，与 iced 同口径）。
const BUTTON_W: f32 = 28.0;
const BUTTON_H: f32 = 20.0;

pub fn show(ui: &egui::Ui, rect: Rect, model: &VideoTimelineModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    // 与 iced `palette.background.weakest` 同源。
    painter.rect_filled(rect, 0, theme::panel_bg());

    let pps = PIXELS_PER_SEC * model.zoom;
    if pps <= 0.0 {
        return;
    }
    // 标尺：刻度贴底，主 12px / 次 6px。
    let start_sec = (model.scroll_x / pps).floor().max(0.0);
    let end_sec = ((model.scroll_x + rect.width()) / pps).min(model.content_secs());
    let mut t = start_sec;
    while t <= end_sec {
        let is_major = (t % MAJOR_INTERVAL).abs() < 0.01;
        let tick_h = if is_major { 12.0 } else { 6.0 };
        let x = rect.left() + t * pps - model.scroll_x;
        if x >= rect.left() - 1.0 && x <= rect.right() + 1.0 {
            painter.rect_filled(
                Rect::from_min_size(
                    egui::pos2(x, rect.top() + RULER_HEIGHT - tick_h),
                    egui::vec2(1.0, tick_h),
                ),
                0,
                if is_major {
                    theme::selected_bg()
                } else {
                    theme::toolbar_bg()
                },
            );
            if is_major {
                painter.text(
                    egui::pos2(x + 3.0, rect.top() + 2.0),
                    Align2::LEFT_TOP,
                    format!("{t:.0}s"),
                    FontId::proportional(10.0),
                    theme::text_muted(),
                );
            }
        }
        t += MINOR_INTERVAL;
    }

    // 双轨条。
    for (track, edit, color, label) in [
        (
            ClipTrack::Video,
            model.video,
            egui::Color32::from_rgb(51, 153, 242),
            "视频",
        ),
        (
            ClipTrack::Audio,
            model.audio,
            egui::Color32::from_rgb(64, 191, 89),
            "音频",
        ),
    ] {
        let geom = TrackGeom::new(
            edit.span(model.duration_secs),
            pps,
            model.scroll_x,
            TrackGeom::track_y(track),
        );
        if geom.w <= 0.0 || geom.x > rect.width() || geom.x + geom.w < 0.0 {
            continue;
        }
        let body = Rect::from_min_size(
            egui::pos2(rect.left() + geom.x, rect.top() + geom.y),
            egui::vec2(geom.w, TRACK_HEIGHT),
        );
        painter.rect_filled(body, CornerRadius::same(TRACK_CORNER_RADIUS as u8), color);
        // 首尾把手：45% 白，8×40，y+4，圆角 2。
        for hx in [geom.x, geom.x + geom.w - HANDLE_WIDTH] {
            painter.rect_filled(
                Rect::from_min_size(
                    egui::pos2(rect.left() + hx, rect.top() + geom.y + 4.0),
                    egui::vec2(HANDLE_WIDTH, TRACK_HEIGHT - 8.0),
                ),
                CornerRadius::same(2),
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 115),
            );
        }
        // 标签：x+10、垂直居中、白色 11px。
        let label_x = geom.x + 10.0;
        if label_x > -60.0 && label_x < rect.width() {
            painter.text(
                egui::pos2(
                    rect.left() + label_x,
                    rect.top() + geom.y + (TRACK_HEIGHT - 14.0) / 2.0,
                ),
                Align2::LEFT_TOP,
                label,
                FontId::proportional(11.0),
                egui::Color32::WHITE,
            );
        }
    }

    // 走带线：宽 2，纵贯全高。
    let x = rect.left() + model.playhead_secs * pps - model.scroll_x;
    if x >= rect.left() - 2.0 && x <= rect.right() + 2.0 {
        painter.rect_filled(
            Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(2.0, rect.height())),
            0,
            egui::Color32::from_rgb(242, 64, 64),
        );
    }
}

/// 裁剪端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipEdge {
    /// 首端。
    Start,
    /// 尾端。
    End,
}

/// 时间轴交互动作（egui 输入 → 生产侧映射成 `VideoClipAction`）。
///
/// 拖动/裁剪都产出**绝对值**（不做增量累加），与 iced 的
/// `ClipTrackOffsetChanged` / `ClipTrimChanged` 语义一致，避免漂移。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoTimelineAction {
    /// 定位播放头（秒）。
    Seek(f32),
    /// 素材整体偏移改为该值（秒）。
    SetOffset {
        /// 目标轨道。
        track: ClipTrack,
        /// 新偏移。
        offset: f32,
    },
    /// 裁剪量改为该值（秒）。
    SetTrim {
        /// 目标轨道。
        track: ClipTrack,
        /// 裁剪端。
        edge: ClipEdge,
        /// 新裁剪量。
        secs: f32,
    },
    /// 滚轮缩放。
    Zoom {
        /// 缩放因子。
        factor: f32,
        /// 锚点比例。
        fixed_ratio: f32,
    },
    /// 水平滚动（像素）。
    ScrollX(f32),
    /// 播放 / 暂停（剪辑面板独立传输时钟）。
    PlayToggled,
    /// 回到起点。
    Rewound,
}

/// 拖拽会话（跨帧保存）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragSession {
    /// 起始区。
    pub zone: HitZone,
    /// 起始指针局部坐标。
    pub origin: egui::Pos2,
    /// 基准值（偏移 / trim_in / trim_out，秒）。
    pub base: f32,
}

/// 按下：按命中区产出动作并返回拖拽会话（标尺/把手/条身都需要后续移动）。
pub fn press(
    local: egui::Pos2,
    model: &VideoTimelineModel,
    width: f32,
) -> (Option<VideoTimelineAction>, Option<DragSession>) {
    let Some(zone) = hit_test(local, model, width) else {
        return (None, None);
    };
    let pps = (PIXELS_PER_SEC * model.zoom).max(1e-6);
    let secs_at = |x: f32| (x + model.scroll_x) / pps;
    let edit = |track: ClipTrack| match track {
        ClipTrack::Video => model.video,
        ClipTrack::Audio => model.audio,
    };
    match zone {
        HitZone::Ruler => (
            Some(VideoTimelineAction::Seek(secs_at(local.x).max(0.0))),
            Some(DragSession {
                zone,
                origin: local,
                base: 0.0,
            }),
        ),
        HitZone::Body(track) => (
            None,
            Some(DragSession {
                zone,
                origin: local,
                base: edit(track).offset,
            }),
        ),
        HitZone::Head(track) => (
            None,
            Some(DragSession {
                zone,
                origin: local,
                base: edit(track).trim_in,
            }),
        ),
        HitZone::Tail(track) => (
            None,
            Some(DragSession {
                zone,
                origin: local,
                base: edit(track).trim_out,
            }),
        ),
    }
}

/// 拖动：按会话类型产出增量动作（秒域）。
pub fn drag(
    local: egui::Pos2,
    session: DragSession,
    model: &VideoTimelineModel,
) -> Option<VideoTimelineAction> {
    let pps = (PIXELS_PER_SEC * model.zoom).max(1e-6);
    let dx_secs = (local.x - session.origin.x) / pps;
    match session.zone {
        HitZone::Ruler => Some(VideoTimelineAction::Seek(
            ((local.x + model.scroll_x) / pps).max(0.0),
        )),
        HitZone::Body(track) => Some(VideoTimelineAction::SetOffset {
            track,
            offset: (session.base + dx_secs).max(0.0),
        }),
        HitZone::Head(track) => Some(VideoTimelineAction::SetTrim {
            track,
            edge: ClipEdge::Start,
            secs: (session.base + dx_secs)
                .clamp(0.0, (model.duration_secs - model.video.trim_out).max(0.0)),
        }),
        HitZone::Tail(track) => Some(VideoTimelineAction::SetTrim {
            track,
            edge: ClipEdge::End,
            secs: (session.base - dx_secs)
                .clamp(0.0, (model.duration_secs - model.video.trim_in).max(0.0)),
        }),
    }
}

/// 滚轮：Ctrl 缩放、Shift/普通横向滚动（与卷帘/剪辑带同口径）。
pub fn wheel(
    local: egui::Pos2,
    lines: f32,
    ctrl: bool,
    shift: bool,
    width: f32,
) -> Option<VideoTimelineAction> {
    let step = lines.clamp(-1.0, 1.0);
    if step.abs() < f32::EPSILON {
        return None;
    }
    if ctrl {
        Some(VideoTimelineAction::Zoom {
            factor: 1.0 + step * 0.1,
            fixed_ratio: if width > 0.0 {
                (local.x / width).clamp(0.0, 1.0)
            } else {
                0.5
            },
        })
    } else {
        let _ = shift;
        Some(VideoTimelineAction::ScrollX(-step * 60.0))
    }
}

/// 交互会话在 ctx-data 里的键。
const SESSION_ID: &str = "lumino.video-timeline.drag";

/// 处理本帧输入，返回动作（矩形与绘制同源，命中不会错位）。
pub fn interact(ui: &egui::Ui, rect: Rect, model: &VideoTimelineModel) -> Vec<VideoTimelineAction> {
    let (pointer, pressed, released, down) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.any_pressed() && i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.primary_down(),
        )
    });
    let mut actions = Vec::new();

    // 滚轮：Ctrl 缩放 / 其它横向滚动。
    if let Some(pos) = pointer
        && rect.contains(pos)
        && let Some(delta) = wheel_lines(ui)
        && let Some(action) = wheel(
            egui::pos2(pos.x - rect.left(), pos.y - rect.top()),
            delta,
            ui.input(|i| i.modifiers.ctrl),
            ui.input(|i| i.modifiers.shift),
            rect.width(),
        )
    {
        actions.push(action);
        return actions;
    }

    let local = pointer.map(|p| egui::pos2(p.x - rect.left(), p.y - rect.top()));

    // 按下：开拖拽会话（标尺按下同时 Seek）。
    if pressed
        && let Some(local) = local
        && let Some(pos) = pointer
        && rect.contains(pos)
    {
        let (action, session) = press(local, model, rect.width());
        if let Some(session) = session {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(SESSION_ID), session));
        }
        if let Some(action) = action {
            actions.push(action);
        }
        return actions;
    }

    // 拖动：按会话类型产出绝对动作；Seek 在秒值不变时不重复上报。
    if down
        && let Some(local) = local
        && let Some(session) = ui
            .ctx()
            .data(|d| d.get_temp::<DragSession>(egui::Id::unique(SESSION_ID)))
        && let Some(action) = drag(local, session, model)
    {
        let skip = matches!(action, VideoTimelineAction::Seek(secs)
            if ui.ctx().data(|d| d.get_temp::<f32>(egui::Id::unique("lumino.video-timeline.seek"))) == Some(secs));
        if !skip {
            if let VideoTimelineAction::Seek(secs) = action {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique("lumino.video-timeline.seek"), secs)
                });
            }
            actions.push(action);
        }
        return actions;
    }

    // 释放：结束会话。
    if released {
        ui.ctx()
            .data_mut(|d| d.remove::<DragSession>(egui::Id::unique(SESSION_ID)));
    }
    actions
}

/// 本帧滚轮刻度（Line 原样、Point / 50）。
fn wheel_lines(ui: &egui::Ui) -> Option<f32> {
    let mut out = None;
    ui.input(|i| {
        for event in &i.raw.events {
            if let egui::Event::MouseWheel { unit, delta, .. } = event {
                out = Some(match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Point => delta.y / 50.0,
                    egui::MouseWheelUnit::Page => delta.y * 10.0,
                });
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> VideoTimelineModel {
        VideoTimelineModel {
            duration_secs: 30.0,
            zoom: 1.0,
            scroll_x: 0.0,
            playhead_secs: 0.0,
            video: ClipEdit::default(),
            audio: ClipEdit::default(),
            viewport_w: 400.0,
            title_row: None,
            scrollbar: None,
            content_w: 0.0,
            title: "剪辑带".to_string(),
            info: String::new(),
            playing: false,
        }
    }

    #[test]
    fn press_on_ruler_seeks_and_opens_session() {
        let m = model();
        let (action, session) = press(egui::pos2(80.0, 10.0), &m, 400.0);
        // 80px / 80pps = 1s。
        assert!(
            matches!(action, Some(VideoTimelineAction::Seek(s)) if (s - 1.0).abs() < 1e-3),
            "标尺按下应 Seek 到 1s，实际 {action:?}"
        );
        assert!(matches!(session.map(|s| s.zone), Some(HitZone::Ruler)));
    }

    #[test]
    fn drag_uses_zone_semantics_with_absolute_values() {
        let m = model();
        let y_body = TrackGeom::track_y(ClipTrack::Video) + TRACK_HEIGHT / 2.0;
        // 条身拖动：基准 offset=0，右移 160px = +2s → 绝对 offset=2s。
        let session = DragSession {
            zone: HitZone::Body(ClipTrack::Video),
            origin: egui::pos2(300.0, y_body),
            base: 0.0,
        };
        let action = drag(egui::pos2(460.0, y_body), session, &m);
        assert!(
            matches!(action, Some(VideoTimelineAction::SetOffset { track, offset })
                if track == ClipTrack::Video && (offset - 2.0).abs() < 1e-3),
            "条身拖动应产出绝对 offset=+2s，实际 {action:?}"
        );

        // 起始把手：基准 trim_in=1s，右移 80px = +1s → trim_in=2s。
        let head = DragSession {
            zone: HitZone::Head(ClipTrack::Video),
            origin: egui::pos2(8.0, y_body),
            base: 1.0,
        };
        let action = drag(egui::pos2(88.0, y_body), head, &m);
        assert!(
            matches!(action, Some(VideoTimelineAction::SetTrim { edge, secs, .. })
                if edge == ClipEdge::Start && (secs - 2.0).abs() < 1e-3),
            "起始把手应产出 trim_in=2s，实际 {action:?}"
        );

        // 结束把手：基准 trim_out=0，左移 80px → trim_out=+1s。
        let tail = DragSession {
            zone: HitZone::Tail(ClipTrack::Video),
            origin: egui::pos2(300.0, y_body),
            base: 0.0,
        };
        let action = drag(egui::pos2(220.0, y_body), tail, &m);
        assert!(
            matches!(action, Some(VideoTimelineAction::SetTrim { edge, secs, .. })
                if edge == ClipEdge::End && (secs - 1.0).abs() < 1e-3),
            "结束把手左移应产出 trim_out=1s，实际 {action:?}"
        );
    }

    #[test]
    fn wheel_ctrl_zooms_otherwise_scrolls() {
        let zoom = wheel(egui::pos2(100.0, 10.0), 1.0, true, false, 400.0);
        assert!(
            matches!(zoom, Some(VideoTimelineAction::Zoom { factor, fixed_ratio })
                if (factor - 1.1).abs() < 1e-4 && (fixed_ratio - 0.25).abs() < 1e-4),
            "Ctrl+滚轮应产出 ×1.1、锚点 0.25，实际 {zoom:?}"
        );
        let scroll = wheel(egui::pos2(100.0, 10.0), 1.0, false, false, 400.0);
        assert!(
            matches!(scroll, Some(VideoTimelineAction::ScrollX(d)) if (d + 60.0).abs() < 1e-3),
            "普通滚轮应横向滚动 -60px，实际 {scroll:?}"
        );
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
        let g = TrackGeom::new((10.0, 5.0), 160.0, 100.0, 0.0);
        assert!((g.x - (10.0 * 160.0 - 100.0)).abs() < 0.01);
        assert!((g.w - 5.0 * 160.0).abs() < 0.01);
    }
}
