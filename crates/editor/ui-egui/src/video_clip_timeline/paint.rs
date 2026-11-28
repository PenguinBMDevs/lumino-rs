//! 时间轴绘制：标题行（标题 + 传输按钮 + 信息）与标尺 / 双轨条 / 走带线。
//!
//! 只读模型、只画像素；按钮点击以 [`VideoTimelineAction`] 推入通道，不改状态。

use crate::text::text_width;
use crate::theme;
use egui::{Align2, CornerRadius, FontId, Rect, Vec2};

use super::input::VideoTimelineAction;
use super::model::{
    ClipTrack, HANDLE_WIDTH, MAJOR_INTERVAL, MINOR_INTERVAL, PIXELS_PER_SEC, RULER_HEIGHT,
    TRACK_CORNER_RADIUS, TRACK_HEIGHT, TrackGeom, VideoTimelineModel,
};

/// 绘制时间轴（`rect` = 控件区域）。
///
/// 颜色/几何逐项对齐 上一代 `timeline_canvas/draw.rs`：
/// 背景 `weakest`、标尺主刻度 `strong` 12px / 次刻度 `weak` 6px（贴底）、
/// 主刻度标签 10px `weak.text`、视频条 `rgb(0.2,0.6,0.95)`、音频条
/// `rgb(0.25,0.75,0.35)`、把手为 45% 白（8×40，y+4）、标签白色 11px（x+10 居中）、
/// 走带线 `rgb(0.95,0.25,0.25)` 宽 2。
/// 标题行：标题 + 倒带/播放按钮 + 信息文本（全部 egui 自绘，左侧对齐）。
///
/// 与 上一代 原布局同口径：行内左边距 8、间距 4、按钮 28×20（图标 16 + 内边距 6/2）、
/// 标题 12px、信息 11px。行矩形由 上一代 的探测占位提供，跨框架不存在文本度量对齐。
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
    let rewind_resp = crate::widgets::hover_text(rewind_resp, &model.rewind_tooltip);
    let play_resp = crate::widgets::hover_text(
        play_resp,
        if model.playing {
            &model.pause_tooltip
        } else {
            &model.play_tooltip
        },
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

    // 标题行由 egui 独占：上一代 的同位占位行不再需要事件。
    crate::chrome::menu::extend_blocked(ui.ctx(), [rect]);
}

/// 传输按钮尺寸（图标 16 + 内边距 6/2，与 上一代 同口径）。
const BUTTON_W: f32 = 28.0;
const BUTTON_H: f32 = 20.0;

pub fn show(ui: &egui::Ui, rect: Rect, model: &VideoTimelineModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    // 与主题的 `background.weakest` 同源。
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
