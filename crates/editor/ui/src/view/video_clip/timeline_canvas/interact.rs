//! 剪辑带时间轴 Canvas 交互（拖拽会话状态机）
//!
//! 从 `timeline_canvas.rs` 拆出（保持单文件 <400 行约束）：
//! 按下命中分发（素材把手 > 条身 > 标尺 scrub）、拖拽移动的
//! 绝对值消息构造，以及交互纯函数测试。

use iced_core::{Point, Rectangle, Vector};
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
    // ⚠️ 坐标域：`pos` / `bounds` 均为**窗口绝对坐标**（iced 契约：
    // `Cursor::position` 返回绝对位置，`Program::update` 收到的是 `layout.bounds()`），
    // 而 [`TrackGeom`] 与 `ruler_click_secs` 全部工作在**画布局部坐标**。
    // 命中测试必须喂局部坐标，否则纵向轨道带判定恒不命中（表现为整条素材带无法拖拽）。
    let local = pos - Vector::new(bounds.x, bounds.y);
    let pps_zoom = PIXELS_PER_SEC * canvas.zoom;
    let grab_secs = ruler_click_secs(local.x, canvas.scroll_x, canvas.zoom, canvas.content_secs());

    // 素材条命中（把手优先于条身）
    let (track, zone) = hit_track(canvas, local, pps_zoom);
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
    let local = pos - Vector::new(bounds.x, bounds.y);
    let local_x = local.x.clamp(0.0, bounds.width);
    let cur_secs = ruler_click_secs(local_x, canvas.scroll_x, canvas.zoom, canvas.content_secs());
    let delta = cur_secs - drag.grab_secs;
    match drag.mode {
        HitZone::Body => {
            // 整体移动：只发**绝对目标值**，边界由状态层 `set_offset` 单独裁决
            // （不变量 = 可视左缘不越过时间轴原点 → offset ≥ −trim_in）。
            // ⚠️ 不要在画布层再钳一次 `max(0.0)`：那会把不变量错安在「素材自身
            // 原点」上，首端裁短后素材带就永远拖不回时间轴开头。两侧各钳一次
            // 且规则不一致，正是本 bug 的成因。
            Some(offset_action(drag.track, drag.orig_offset + delta))
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
///
/// `local` 必须是**画布局部坐标**（`pos - bounds.origin`）——`TrackGeom` 的
/// x/y 均以画布左上角为原点。传窗口绝对坐标会让纵向轨道带判定整体偏移，
/// 命中恒为 `(None, None)`。
pub(super) fn hit_track(
    canvas: &TimelineCanvas,
    local: Point,
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
        local,
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
