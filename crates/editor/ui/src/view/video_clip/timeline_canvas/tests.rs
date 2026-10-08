//! TimelineCanvas 单测（从 timeline_canvas.rs 拆出）

use super::*;

/// 测试用时间轴画布（默认素材态：无偏移、无裁剪）
fn canvas_fixture(duration_secs: f32) -> TimelineCanvas {
    TimelineCanvas {
        duration_secs,
        zoom: 1.0,
        scroll_x: 0.0,
        ctrl_pressed: false,
        playhead_secs: 0.0,
        is_playing: false,
        video_edit: ClipTrackEdit::default(),
        audio_edit: ClipTrackEdit::default(),
    }
}

/// 生产布局下的画布绝对矩形：左侧 12 padding + 220 轨道面板 + 12 行间距
/// + 时间轴容器 8 padding → `x = 252`；y 为其上方 header/预览区之和。
fn layout_bounds() -> Rectangle {
    Rectangle {
        x: 252.0,
        y: 412.0,
        width: 1200.0,
        height: 180.0,
    }
}

#[test]
fn test_pure_functions() {
    // 滚轮钳制
    assert_eq!(wheel_scroll_delta(100.0), SCROLL_MAX_DELTA);
    assert_eq!(wheel_scroll_delta(-1.0), -SCROLL_LINES_SCALE);
    // 标尺点击换算：x=400 无滚动 zoom=1 → 5s；越界钳到时长
    assert!((ruler_click_secs(400.0, 0.0, 1.0, 30.0) - 5.0).abs() < f32::EPSILON);
    assert!((ruler_click_secs(9999.0, 0.0, 1.0, 30.0) - 30.0).abs() < f32::EPSILON);
}

#[test]
fn test_playhead_screen_x_follow_and_cull() {
    let c = |scroll: f32, playhead: f32| TimelineCanvas {
        duration_secs: 30.0,
        zoom: 1.0,
        scroll_x: scroll,
        ctrl_pressed: false,
        playhead_secs: playhead,
        is_playing: false,
        video_edit: ClipTrackEdit::default(),
        audio_edit: ClipTrackEdit::default(),
    };
    // 播放头 5s 内容 x=400，跟随滚动 350 → 屏幕 x 恒为 PLAYHEAD_X
    let x = c(350.0, 5.0).playhead_screen_x(800.0).expect("应在视口内");
    assert!((x - crate::view::video_clip::layout::PLAYHEAD_X).abs() < f32::EPSILON);
    // 手动滚远后移出视口 → 不绘制
    assert!(c(2000.0, 5.0).playhead_screen_x(800.0).is_none());
}

/// 回归：画布不在窗口原点时，命中测试必须把**窗口绝对坐标**换算为
/// **画布局部坐标**再比对 `TrackGeom`。
///
/// iced 契约：`Cursor::position` 与 `Program::update` 的 `bounds` 都是绝对坐标
/// （`layout.bounds()`），而 `TrackGeom` 以画布左上角为原点。混用会让纵向
/// 轨道带判定整体偏移，命中恒为 `(None, None)`——表现就是
/// 「素材带边缘拖不动（其实是整条带都拖不动）」。
#[test]
fn test_begin_drag_hits_track_with_layout_offset() {
    use iced_core::Point;
    use lumino_message::video_clip::ClipTrack;

    let canvas = canvas_fixture(30.0);
    let bounds = layout_bounds();
    let mut state = TimelineDragState::default();

    // 视频轨首端把手：局部 (2, 40) → 绝对 (254, 452)
    let press = interact::begin_drag(&canvas, &mut state, Point::new(254.0, 452.0), bounds);
    assert!(press.is_some(), "带布局偏移时按下首端把手必须命中");
    assert_eq!(
        state.track_drag.map(|d| d.mode),
        Some(HitZone::HandleStart),
        "首端把手必须命中 HandleStart（而非被误判为条身/未命中）"
    );

    // 尾端把手：局部 x = 30s×80px/s = 2400 附近
    state.track_drag = None;
    let press_end = interact::begin_drag(
        &canvas,
        &mut state,
        Point::new(bounds.x + 2398.0, 452.0),
        bounds,
    );
    assert!(press_end.is_some(), "带布局偏移时按下尾端把手必须命中");
    assert_eq!(state.track_drag.map(|d| d.mode), Some(HitZone::HandleEnd));

    // 条身：整体移动仍然可用
    state.track_drag = None;
    let press_body = interact::begin_drag(
        &canvas,
        &mut state,
        Point::new(bounds.x + 400.0, 452.0),
        bounds,
    );
    assert!(press_body.is_some(), "带布局偏移时条身必须命中");
    assert_eq!(state.track_drag.map(|d| d.mode), Some(HitZone::Body));

    // 音频轨（局部 y ∈ [88,136]）
    state.track_drag = None;
    let press_audio = interact::begin_drag(
        &canvas,
        &mut state,
        Point::new(bounds.x + 400.0, bounds.y + 100.0),
        bounds,
    );
    assert!(press_audio.is_some(), "带布局偏移时音频轨必须命中");
    assert_eq!(state.track_drag.map(|d| d.track), Some(ClipTrack::Audio));

    // 素材带之外（局部 x 越界）不得命中：防止「点到空白也起拖」
    state.track_drag = None;
    let miss = interact::begin_drag(
        &canvas,
        &mut state,
        Point::new(bounds.x + 2500.0, 452.0),
        bounds,
    );
    assert!(miss.is_none(), "素材带右缘之外不应起拖");
}

/// 回归：指针在画布外松手必须结束拖拽会话。
///
/// 收尾逻辑若放在 `bounds.contains` 早退之后，`track_drag` 会残留——
/// 之后**未按键**的鼠标移动仍持续发裁剪消息，素材带自己跟着指针跑
/// （用户感知为「松手了还在动」）。
#[test]
fn test_release_outside_canvas_ends_drag_session() {
    use iced_core::Point;
    use iced_core::mouse;
    use iced_widget::canvas::{self, Program};

    let canvas = canvas_fixture(30.0);
    let bounds = layout_bounds();
    let mut state = TimelineDragState::default();
    let inside = mouse::Cursor::Available(Point::new(254.0, 452.0));

    let press = canvas.update(
        &mut state,
        &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        inside,
    );
    assert!(press.is_some(), "按下首端把手应命中并起拖");
    assert!(state.track_drag.is_some(), "拖拽会话应已建立");

    // 指针拖到画布外（面板下方）再松手
    let outside = mouse::Cursor::Available(Point::new(600.0, 700.0));
    let released = canvas.update(
        &mut state,
        &canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        bounds,
        outside,
    );
    assert!(released.is_some(), "画布外松手应消费事件并收尾");
    assert!(state.track_drag.is_none(), "松手后会话必须清空，不得残留");
    assert!(!state.scrubbing);

    // 残留检测：会话结束后未按键的移动不得再产生任何素材带变更
    let moved = canvas.update(
        &mut state,
        &canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(700.0, 700.0),
        }),
        bounds,
        outside,
    );
    assert!(moved.is_none(), "会话已结束，移动不得再改素材带");
}

#[test]
fn test_content_width_scales_with_zoom_and_min() {
    let mut c = TimelineCanvas {
        duration_secs: 10.0,
        zoom: 1.0,
        scroll_x: 0.0,
        ctrl_pressed: false,
        playhead_secs: 0.0,
        is_playing: false,
        video_edit: ClipTrackEdit::default(),
        audio_edit: ClipTrackEdit::default(),
    };
    // 10s * 80px/s = 800
    assert!((c.content_width() - 800.0).abs() < f32::EPSILON);
    c.duration_secs = 0.0;
    // 最小兜底 400
    assert!((c.content_width() - 400.0).abs() < f32::EPSILON);
}

/// 实证：按下视频轨首端把手并向右拖动，必须发出 `ClipTrimChanged::Start`，
/// 且裁剪值 ≈ 拖动秒数，可见长度随之缩短。
///
/// 全部坐标使用**窗口绝对坐标 + 生产布局偏移**（`layout_bounds`），
/// 与 iced 在真实运行时喂进来的坐标域完全一致。
#[test]
fn test_drag_start_handle_trims_and_shortens_strip() {
    use crate::message::VideoClipAction;
    use iced_core::Point;
    use lumino_message::video_clip::{ClipTrack, ClipTrimEdge};

    let canvas = canvas_fixture(30.0);
    let bounds = layout_bounds();
    let mut state = TimelineDragState::default();
    // 按下视频轨首端把手：局部 x=2（落在左缘 HANDLE_WIDTH 内），局部 y=40（视频轨 y∈[32,80]）
    let press = interact::begin_drag(
        &canvas,
        &mut state,
        Point::new(bounds.x + 2.0, bounds.y + 40.0),
        bounds,
    );
    assert!(press.is_some(), "按下把手应返回动作");
    assert!(state.track_drag.is_some(), "拖拽会话应已建立");

    // 拖到局部 x=160（2s * 80px/s）处
    let drag = state.track_drag.expect("前面已断言拖拽会话存在，应能取出");
    let action = interact::drag_move(
        &canvas,
        Point::new(bounds.x + 160.0, bounds.y + 40.0),
        bounds,
        drag,
    );
    let (msg, _redraw, _status) = action.expect("拖拽应返回动作").into_inner();
    match msg {
        Some(Message::VideoClip(VideoClipAction::ClipTrimChanged {
            track,
            edge,
            trim_secs,
        })) => {
            assert_eq!(track, ClipTrack::Video);
            assert_eq!(edge, ClipTrimEdge::Start);
            assert!(
                (trim_secs - 2.0).abs() < 0.5,
                "首端裁剪应≈2s（拖到 2s 处），实际 {trim_secs}"
            );
        }
        other => panic!("期望 ClipTrimChanged::Start 发布，实际: {other:?}"),
    }
}

/// 实证：按下视频轨尾端把手并向左拖动，必须发出 `ClipTrimChanged::End`，
/// 且裁剪值 ≈（源长 − 拖动秒数），可见长度随之缩短。
#[test]
fn test_drag_end_handle_trims_and_shortens_strip() {
    use crate::message::VideoClipAction;
    use iced_core::Point;
    use lumino_message::video_clip::{ClipTrack, ClipTrimEdge};

    let canvas = canvas_fixture(30.0);
    // 宽视口：zoom=1 时素材带 30s×80px/s = 2400px，两端把手同屏可见
    let bounds = Rectangle {
        width: 2600.0,
        ..layout_bounds()
    };
    let mut state = TimelineDragState::default();
    // 尾端把手在局部 x = 30s×80px/s = 2400 处
    let right_edge_x = 30.0 * PIXELS_PER_SEC;
    let press = interact::begin_drag(
        &canvas,
        &mut state,
        Point::new(bounds.x + right_edge_x - 2.0, bounds.y + 40.0),
        bounds,
    );
    assert!(press.is_some(), "按下尾端把手应返回动作");
    assert_eq!(
        state.track_drag.map(|d| d.mode),
        Some(HitZone::HandleEnd),
        "尾端把手必须命中 HandleEnd"
    );
    let drag = state.track_drag.expect("前面已断言拖拽会话存在，应能取出");
    // 向左拖到局部 x=1600（20s）→ 右缘应=20s，尾裁=30−20=10s
    let action = interact::drag_move(
        &canvas,
        Point::new(bounds.x + 1600.0, bounds.y + 40.0),
        bounds,
        drag,
    );
    let (msg, _redraw, _status) = action.expect("拖拽应返回动作").into_inner();
    match msg {
        Some(Message::VideoClip(VideoClipAction::ClipTrimChanged {
            track,
            edge,
            trim_secs,
        })) => {
            assert_eq!(track, ClipTrack::Video);
            assert_eq!(edge, ClipTrimEdge::End);
            assert!(
                (trim_secs - 10.0).abs() < 0.5,
                "尾端裁剪应≈10s（右缘拖到 20s），实际 {trim_secs}"
            );
        }
        other => panic!("期望 ClipTrimChanged::End 发布，实际: {other:?}"),
    }
}

/// 时间轴内容宽度必须容纳被整体右移的素材带（拖出去还得滚得回来）。
#[test]
fn test_content_width_covers_shifted_strip() {
    let mut c = canvas_fixture(10.0);
    assert!((c.content_width() - 800.0).abs() < f32::EPSILON);
    // 视频带整体右移 4s → 右缘 14s，内容宽随之扩到 14×80
    c.video_edit.set_offset(4.0);
    assert!((c.content_secs() - 14.0).abs() < 1e-4);
    assert!((c.content_width() - 1120.0).abs() < 1e-3);
    // 标尺/播放头换算的上限同源，越界位置不得被当作可定位点
    assert!((ruler_click_secs(9999.0, 0.0, 1.0, c.content_secs()) - 14.0).abs() < 1e-3);
}

/// 回归：条身拖拽只发**绝对目标值**，不得在画布层再钳一次 `max(0.0)`。
///
/// 首端裁掉 5s 后，整体向前拖的目标偏移必然是负数（合法下限是 −首裁）。
/// 画布若按旧规则钳成 ≥0，素材带就永远拖不回时间轴开头——边界必须由
/// 状态层 `set_offset` 单独裁决，两侧各钳一次且规则不一致就是本 bug 的成因。
#[test]
fn test_body_drag_publishes_raw_offset_without_canvas_clamp() {
    use crate::message::VideoClipAction;
    use iced_core::Point;
    use lumino_message::video_clip::ClipTrack;

    let mut canvas = canvas_fixture(30.0);
    // 素材带：整体偏移 5s + 首端裁 5s → 可视 [10, 30]，宽 20s×80px = 1600px
    canvas.video_edit.offset_secs = 5.0;
    canvas.video_edit.trim_in_secs = 5.0;
    let bounds = layout_bounds();
    let mut state = TimelineDragState::default();

    // 按住条身中部（局部 x = 1600）起拖
    let press = interact::begin_drag(
        &canvas,
        &mut state,
        Point::new(bounds.x + 1600.0, bounds.y + 40.0),
        bounds,
    );
    assert!(press.is_some(), "条身应命中");
    assert_eq!(state.track_drag.map(|d| d.mode), Some(HitZone::Body));
    let drag = state.track_drag.expect("会话已建立");

    // 向前拖到局部 x=1000（12.5s）→ delta = −7.5s → 目标偏移 = 5 − 7.5 = −2.5
    let action = interact::drag_move(
        &canvas,
        Point::new(bounds.x + 1000.0, bounds.y + 40.0),
        bounds,
        drag,
    );
    let (msg, _redraw, _status) = action.expect("拖拽应返回动作").into_inner();
    match msg {
        Some(Message::VideoClip(VideoClipAction::ClipTrackOffsetChanged {
            track,
            offset_secs,
        })) => {
            assert_eq!(track, ClipTrack::Video);
            assert!(
                (offset_secs + 2.5).abs() < 0.1,
                "条身拖拽必须原样发出负偏移（合法性由状态层裁决），实际 {offset_secs}"
            );
        }
        other => panic!("期望 ClipTrackOffsetChanged 发布，实际: {other:?}"),
    }
}
