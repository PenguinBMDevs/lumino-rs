//! 时间轴输入：命中区 → 动作，以及跨帧拖拽会话。
//!
//! 拖动/裁剪产出的都是**绝对值**，会话存 `ctx.data`，由 [`interact`] 维护生命周期；
//! 本片不画像素，绘制见 `paint`。

use super::model::{ClipTrack, HitZone, PIXELS_PER_SEC, VideoTimelineModel, hit_test};

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
/// 拖动/裁剪都产出**绝对值**（不做增量累加），与 上一代 的
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
pub fn interact(
    ui: &egui::Ui,
    rect: egui::Rect,
    model: &VideoTimelineModel,
) -> Vec<VideoTimelineAction> {
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
