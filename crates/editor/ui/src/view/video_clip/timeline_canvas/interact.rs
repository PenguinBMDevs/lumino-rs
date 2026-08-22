//! 剪辑带时间轴 Canvas 交互（拖拽会话状态机）
//!
//! 从 `timeline_canvas.rs` 拆出（保持单文件 <400 行约束）：
//! 按下命中分发（素材把手 > 条身 > 标尺 scrub）、拖拽移动的
//! 绝对值消息构造，以及交互纯函数测试。

use iced_core::{Point, Rectangle};
use iced_widget::canvas;

use lumino_message::video_clip::{ClipTrack, ClipTrimEdge};
use lumino_ui_core::state::video_clip_state::ClipTrackEdit;

use crate::message::{Message, VideoClipAction};
use crate::view::video_clip::timeline_canvas::{
    HitZone, PIXELS_PER_SEC, RULER_HEIGHT, TimelineCanvas, TimelineDragState, ruler_click_secs,
};

/// 素材条拖拽会话（按下时捕获的基准值，motion 发绝对值消息无漂移）
#[derive(Debug, Clone, Copy)]
pub struct TrackDrag {
    /// 目标轨道
    pub track: ClipTrack,
    /// 拖拽模式（移动 / 首裁 / 尾裁）
    pub mode: HitZone,
    /// 按下时指针对应的内容秒
    pub grab_secs: f32,
    /// 按下时的整体偏移
    pub orig_offset: f32,
    /// 按下时的首端裁剪
    pub orig_trim_in: f32,
    /// 按下时的尾端裁剪
    pub orig_trim_out: f32,
}

/// 素材条拖拽会话（按下时捕获的基准值，motion 发绝对值消息无漂移）
pub(super) fn begin_drag(
    canvas: &TimelineCanvas,
    state: &mut TimelineDragState,
    pos: Point,
    bounds: Rectangle,
) -> Option<canvas::Action<Message>> {
    let local_x = pos.x - bounds.x;
    let pps_zoom = PIXELS_PER_SEC * canvas.zoom;
    let grab_secs = ruler_click_secs(local_x, canvas.scroll_x, canvas.zoom, canvas.duration_secs);

    // 素材条命中（把手优先于条身）
    let (track, zone) = hit_track(canvas, pos, pps_zoom);
    if let (Some(track), Some(zone)) = (track, zone) {
        let edit = edit_of(canvas, track);
        state.track_drag = Some(TrackDrag {
            track,
            mode: zone,
            grab_secs,
            orig_offset: edit.offset_secs,
            orig_trim_in: edit.trim_in_secs,
            orig_trim_out: edit.trim_out_secs,
        });
        return match zone {
            HitZone::HandleStart => {
                Some(trim_action(track, ClipTrimEdge::Start, edit.trim_in_secs).and_capture())
            }
            HitZone::HandleEnd => {
                Some(trim_action(track, ClipTrimEdge::End, edit.trim_out_secs).and_capture())
            }
            HitZone::Body => Some(offset_action(track, edit.offset_secs).and_capture()),
        };
    }

    // 标尺 scrub（播放中禁用）
    if pos.y - bounds.y <= RULER_HEIGHT && !canvas.is_playing {
        state.scrubbing = true;
        return Some(seek_action(grab_secs).and_capture());
    }
    None
}

/// 拖拽移动：按模式计算绝对值并发消息（无漂移）
pub(super) fn drag_move(
    canvas: &TimelineCanvas,
    pos: Point,
    bounds: Rectangle,
    drag: TrackDrag,
) -> Option<canvas::Action<Message>> {
    let local_x = (pos.x - bounds.x).clamp(0.0, bounds.width);
    let cur_secs = ruler_click_secs(local_x, canvas.scroll_x, canvas.zoom, canvas.duration_secs);
    let delta = cur_secs - drag.grab_secs;
    match drag.mode {
        HitZone::Body => {
            // 偏移下限 0（负值由 handler set_offset 兜底钳制）
            Some(offset_action(
                drag.track,
                (drag.orig_offset + delta).max(0.0),
            ))
        }
        HitZone::HandleStart => {
            // 向右拖首端把手 = 裁掉更多；负值由 handler 钳制到 0
            let trim_in = drag.orig_trim_in + delta;
            Some(trim_action(drag.track, ClipTrimEdge::Start, trim_in))
        }
        HitZone::HandleEnd => {
            // 向左拖尾端把手 = 裁掉更多
            let trim_out = drag.orig_trim_out - delta;
            Some(trim_action(drag.track, ClipTrimEdge::End, trim_out))
        }
    }
}

/// 命中测试两条轨道（把手优先于条身）
pub(super) fn hit_track(
    canvas: &TimelineCanvas,
    pos: Point,
    pps_zoom: f32,
) -> (Option<ClipTrack>, Option<HitZone>) {
    use crate::view::video_clip::timeline_canvas::{TrackGeom, draw};
    draw::hit_test_track(
        TrackGeom::new(
            &canvas.video_edit,
            canvas.duration_secs,
            pps_zoom,
            canvas.scroll_x,
            TrackGeom::track_y(ClipTrack::Video),
        ),
        TrackGeom::new(
            &canvas.audio_edit,
            canvas.duration_secs,
            pps_zoom,
            canvas.scroll_x,
            TrackGeom::track_y(ClipTrack::Audio),
        ),
        pos,
    )
}

/// 取指定轨道的编辑状态引用
fn edit_of(canvas: &TimelineCanvas, track: ClipTrack) -> &ClipTrackEdit {
    match track {
        ClipTrack::Video => &canvas.video_edit,
        ClipTrack::Audio => &canvas.audio_edit,
    }
}

/// 构造 seek 消息动作（scrub 拖拽复用）
pub(super) fn seek_action(secs: f32) -> canvas::Action<Message> {
    canvas::Action::publish(Message::VideoClip(VideoClipAction::TimelineSeek { secs }))
}

/// 构造素材偏移消息动作
fn offset_action(track: ClipTrack, offset_secs: f32) -> canvas::Action<Message> {
    canvas::Action::publish(Message::VideoClip(
        VideoClipAction::ClipTrackOffsetChanged { track, offset_secs },
    ))
}

/// 构造素材裁剪消息动作（负值交由 handler 钳制到 0）
fn trim_action(track: ClipTrack, edge: ClipTrimEdge, trim_secs: f32) -> canvas::Action<Message> {
    canvas::Action::publish(Message::VideoClip(VideoClipAction::ClipTrimChanged {
        track,
        edge,
        trim_secs,
    }))
}
