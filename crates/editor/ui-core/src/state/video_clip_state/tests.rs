//! 视频剪辑状态单测：缩放 / 平移 / 裁剪 / 传输与时间轴换算。

use super::*;

#[test]
fn test_default_state() {
    let s = VideoClipState::new();
    assert!((s.zoom - 1.0).abs() < f32::EPSILON);
    assert!((s.pan_x).abs() < f32::EPSILON);
}

#[test]
fn test_zoom_clamp() {
    let mut s = VideoClipState::new();
    s.apply_zoom(100.0);
    assert!(s.zoom <= VIDEO_CLIP_MAX_ZOOM);
    s.set_zoom(0.1);
    assert!(s.zoom >= VIDEO_CLIP_MIN_ZOOM);
}

#[test]
fn test_reset() {
    let mut s = VideoClipState::new();
    s.zoom = 5.0;
    s.pan_x = 100.0;
    s.reset_view();
    assert!((s.zoom - 1.0).abs() < f32::EPSILON);
    assert!((s.pan_x).abs() < f32::EPSILON);
}

#[test]
fn test_pan_clamp() {
    let mut s = VideoClipState::new();
    s.zoom = 2.0;
    s.pan_x = 1000.0;
    s.clamp_pan(800.0, 600.0, 400.0, 300.0);
    // scaled 800x600 vs available 800x600 -> excess 0 -> pan 0
    assert!((s.pan_x).abs() < f32::EPSILON);
}

#[test]
fn test_timeline_scroll_clamp() {
    let mut s = VideoClipState::new();
    // 内容 2000 宽、视口 800 → 有效范围 [0, 1200]
    s.set_timeline_scroll(5000.0, 2000.0, 800.0);
    assert!((s.timeline_scroll_x - 1200.0).abs() < f32::EPSILON);
    s.set_timeline_scroll(-100.0, 2000.0, 800.0);
    assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
    // 内容不足视口 → 恒为 0
    s.set_timeline_scroll(50.0, 400.0, 800.0);
    assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
}

#[test]
fn test_timeline_zoom_around_anchor_stays_put() {
    let mut s = VideoClipState::new();
    let base_w = 1600.0; // 未缩放内容宽
    let viewport = 800.0;
    // 先滚到 200，锚点在视口右端（ratio=1.0）
    s.set_timeline_scroll(200.0, base_w, viewport);
    let old_zoom = s.zoom;
    s.timeline_zoom_around(2.0, 1.0, old_zoom, base_w, viewport);
    // 锚点内容坐标：200+800=1000；放大 2 倍后该点仍在视口右端：
    // new_scroll = 1000*2 - 800 = 1200
    assert!((s.timeline_scroll_x - 1200.0).abs() < 0.01);
    assert!((s.zoom - 2.0).abs() < f32::EPSILON);

    // 缩小回 1.0（锚点左端 ratio=0）：左缘内容点 1200 等比映射 → scroll = 1200*0.5 = 600，
    // 且该内容点仍位于视口左缘（锚点不动性的本质）
    s.timeline_zoom_around(1.0, 0.0, s.zoom, base_w, viewport);
    assert!((s.timeline_scroll_x - 600.0).abs() < 0.01);
    assert!((s.zoom - 1.0).abs() < f32::EPSILON);

    // 钳制：继续放大 10 倍时 scroll 不越界
    s.timeline_zoom_around(10.0, 1.0, s.zoom, base_w, viewport);
    let max_scroll = (base_w * 10.0 - viewport).max(0.0);
    assert!(s.timeline_scroll_x <= max_scroll + 0.01);
}

#[test]
fn test_reset_clears_timeline_scroll() {
    let mut s = VideoClipState::new();
    s.timeline_scroll_x = 300.0;
    s.reset_view();
    assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
}

#[test]
fn test_clip_track_edit_visible_window() {
    let mut e = ClipTrackEdit::default();
    // 偏移 2s、首裁 1s、尾裁 0.5s、源长 10s → 可视起点 [3]，可视长 = 10−1−0.5 = 8.5
    e.set_offset(2.0);
    e.set_trim_start(1.0, 10.0);
    e.set_trim_end(0.5, 10.0);
    assert!((e.visible_start() - 3.0).abs() < f32::EPSILON);
    assert!((e.visible_len(10.0) - 8.5).abs() < f32::EPSILON);

    // 首裁钳制上界 = 源长 − 尾裁 − 最小时长 = 10 − 0.5 − 0.1 = 9.4（绝对值 API）
    e.set_trim_start(100.0, 10.0);
    assert!((e.trim_in_secs - 9.4).abs() < f32::EPSILON);

    // 偏移下限 = −首端裁剪：可视左缘顶到时间轴原点即止（此处 9.4s）
    e.set_offset(-999.0);
    assert!((e.offset_secs + 9.4).abs() < 1e-4, "实际 {}", e.offset_secs);
    assert!(
        e.visible_start().abs() < 1e-4,
        "可视左缘必须能顶到时间轴原点，实际 {}",
        e.visible_start()
    );
}

/// 回归：首端被裁短后，整体拖动必须能把素材带带回时间轴开头（顶到 0）。
///
/// 旧实现把偏移钳成 `≥ 0`（不变量错安在「素材自身原点」上），首裁 5s 后
/// 可视左缘永远 ≥5s —— 用户只能拖到"开头缩短到的位置"，顶不到前面。
#[test]
fn test_band_body_can_reach_timeline_origin_after_head_trim() {
    let source = 30.0;
    let mut e = ClipTrackEdit::default();

    // 首端裁掉 5s：可视区间 [5, 30]
    e.set_trim_start(5.0, source);
    assert_eq!((e.visible_start(), e.visible_end(source)), (5.0, 30.0));

    // 整体向前拖：可视左缘必须能到达 0（隐藏开头允许落到时间轴之外）
    e.set_offset(-999.0);
    assert!(
        e.visible_start().abs() < 1e-4,
        "应能顶到时间轴开头，实际左缘 {}",
        e.visible_start()
    );
    assert!(
        (e.visible_end(source) - 25.0).abs() < 1e-4,
        "右缘随整体平移"
    );
    assert!(
        (e.visible_len(source) - 25.0).abs() < 1e-4,
        "整体移动不改变可视长度"
    );

    // 已经顶在原点：首端把手不能再向左"露出"更多开头（原点之前没有位置）
    e.set_trim_start(2.0, source);
    assert!((e.trim_in_secs - 5.0).abs() < 1e-4, "首裁下限 = −offset");
    assert!(e.visible_start().abs() < 1e-4);

    // 未顶原点时首端仍可正常向左扩（回退裁剪）
    e.set_offset(0.0);
    e.set_trim_start(2.0, source);
    assert!((e.trim_in_secs - 2.0).abs() < 1e-4);

    // 音视频双轨共用同一规则：音频带同样能顶到原点
    let mut a = ClipTrackEdit::default();
    a.set_trim_start(5.0, source);
    a.set_offset(-999.0);
    assert!(a.visible_start().abs() < 1e-4);
}

/// 顶到原点后，时间轴 0 处显示的应是**入点那一帧**（源时间 = 播放头 − 偏移）。
#[test]
fn test_origin_position_shows_in_frame_after_sliding_to_start() {
    let source = 30.0;
    let mut s = VideoClipState::new();
    s.video_edit.set_trim_start(5.0, source);
    s.video_edit.set_offset(-999.0);
    s.set_clip_position(0.0);
    assert_eq!(s.video_window(source), (0.0, 25.0));
    assert!(
        (s.video_source_secs_at(source).expect("带内") - 5.0).abs() < 1e-4,
        "时间轴 0 处应显示入点（源 5s）那一帧"
    );
}

/// 长度调整的硬约束：无论怎么拖，可视长度都不小于最小时长。
#[test]
fn test_clip_trim_never_below_min_duration() {
    let source = 10.0;
    // 首端：一路拖到底 → 可视长度正好压在最小时长上
    let mut e = ClipTrackEdit::default();
    e.set_trim_start(999.0, source);
    assert!(
        (e.visible_len(source) - MIN_CLIP_DURATION_SECS).abs() < 1e-4,
        "首端裁到底应停在最小时长，实际 {}",
        e.visible_len(source)
    );
    // 尾端：同样不得把条拖没
    let mut e2 = ClipTrackEdit::default();
    e2.set_trim_end(999.0, source);
    assert!(
        (e2.visible_len(source) - MIN_CLIP_DURATION_SECS).abs() < 1e-4,
        "尾端裁到底应停在最小时长，实际 {}",
        e2.visible_len(source)
    );
    // 两头夹击：首尾交替裁到底，仍不得低于最小时长（约束必须联合成立）
    let mut e3 = ClipTrackEdit::default();
    for _ in 0..8 {
        e3.set_trim_start(999.0, source);
        e3.set_trim_end(999.0, source);
    }
    assert!(
        e3.visible_len(source) >= MIN_CLIP_DURATION_SECS - 1e-4,
        "首尾交替裁剪后可视长度 {} 低于最小时长",
        e3.visible_len(source)
    );
    assert!(e3.trim_in_secs + e3.trim_out_secs <= source + 1e-4);
}

/// 边界约束：裁剪量不得超过素材实际可用范围（另一端裁剪 + 最小时长）。
#[test]
fn test_clip_trim_within_source_range() {
    let source = 10.0;
    let mut e = ClipTrackEdit::default();
    e.set_trim_end(4.0, source);
    // 首端上界 = 10 − 4 − 0.1
    e.set_trim_start(999.0, source);
    assert!((e.trim_in_secs - 5.9).abs() < 1e-4);
    assert!(e.visible_end(source) <= source + 1e-4, "右缘不得越过源长");

    // 源长退化（比最小时长还短）时不得 panic，也不出现负长度
    let mut tiny = ClipTrackEdit::default();
    tiny.set_trim_start(999.0, 0.02);
    tiny.set_trim_end(999.0, 0.02);
    assert!(tiny.visible_len(0.02) >= 0.0);
    assert!(tiny.visible_start() <= tiny.visible_end(0.02) + 1e-6);
}

/// 播放指示线是**自由时间轴游标**：素材带首端裁剪（带向右缩小）后，
/// 播放头必须停在原来的 X 位置，不得被带头部拖着走。
#[test]
fn test_playhead_not_dragged_by_trimming_band_head() {
    let source = 30.0;
    let mut s = VideoClipState::new();
    s.set_clip_position(2.0);

    // 首端裁到 5s：带起点右移到 5s，播放头仍应在 2s
    s.video_edit.set_trim_start(5.0, source);
    assert!(
        (s.clip_position_secs - 2.0).abs() < 1e-4,
        "首端裁剪不得移动播放头，实际 {}",
        s.clip_position_secs
    );
    assert_eq!(s.video_window(source), (5.0, 30.0));
    // 此时播放头已不在带上 → 画面必须判定为「无素材带」
    assert!(!s.position_in_video_window(source));
    assert_eq!(s.video_source_secs_at(source), None);

    // 尾端也裁：播放头依旧不动，仅改变带的可视区间
    s.set_clip_position(25.0);
    s.video_edit.set_trim_end(10.0, source);
    assert!((s.clip_position_secs - 25.0).abs() < 1e-4);
    assert_eq!(s.video_window(source), (5.0, 20.0));
    assert!(!s.position_in_video_window(source));

    // 播放头移到带内 → 恢复显示，且源时间落在可视区间内
    s.set_clip_position(12.0);
    assert!(s.position_in_video_window(source));
    assert!((s.video_source_secs_at(source).expect("带内应给源时间") - 12.0).abs() < 1e-4);

    // 回零回的是时间轴 0，不随带头移动
    s.clip_rewind();
    assert!(s.clip_position_secs.abs() < f32::EPSILON);
}

/// 画面取帧：时间轴时间 → 素材源时间必须扣除整体偏移（否则整体右移后画面错帧）。
#[test]
fn test_video_source_secs_maps_timeline_to_source() {
    let source = 10.0;
    let mut s = VideoClipState::new();
    // 素材带整体右移 4s：时间轴 [4,14] 对应源 [0,10]
    s.video_edit.set_offset(4.0);
    assert_eq!(s.video_window(source), (4.0, 14.0));

    // 带外：3.9s（带前）与 14.1s（带后）
    s.set_clip_position(3.9);
    assert_eq!(s.video_source_secs_at(source), None);
    s.set_clip_position(14.1);
    assert_eq!(s.video_source_secs_at(source), None);

    // 带内：时间轴 7s → 源 3s
    s.set_clip_position(7.0);
    assert!((s.video_source_secs_at(source).expect("带内") - 3.0).abs() < 1e-4);
    // 边界闭包：入点 4s → 源 0；出点 14s → 源 10
    s.set_clip_position(4.0);
    assert!((s.video_source_secs_at(source).expect("入点属带内") - 0.0).abs() < 1e-4);
    s.set_clip_position(14.0);
    assert!((s.video_source_secs_at(source).expect("出点属带内") - 10.0).abs() < 1e-4);

    // 裁剪与偏移叠加：偏移 4s、首裁 2s、尾裁 3s → 时间轴 [6,11]，源 [2,7]
    s.video_edit.set_trim_start(2.0, source);
    s.video_edit.set_trim_end(3.0, source);
    assert_eq!(s.video_window(source), (6.0, 11.0));
    s.set_clip_position(8.0);
    assert!((s.video_source_secs_at(source).expect("带内") - 4.0).abs() < 1e-4);
    s.set_clip_position(5.5);
    assert_eq!(s.video_source_secs_at(source), None);

    // 退化源长（历史裁剪越界）不得 panic，且区间为空 → 恒 None
    let broken = ClipTrackEdit {
        offset_secs: 0.0,
        trim_in_secs: 8.0,
        trim_out_secs: 0.0,
    };
    let mut b = VideoClipState::new();
    b.video_edit = broken;
    b.set_clip_position(9.0);
    assert_eq!(b.video_source_secs_at(3.0), None);
}

/// 时间轴内容长度必须容纳被整体右移的素材带（否则拖出去就滚不回来）。
#[test]
fn test_timeline_content_secs_covers_shifted_strip() {
    let source = 10.0;
    let mut e = ClipTrackEdit::default();
    assert!((timeline_content_secs(source, &[e]) - 10.0).abs() < 1e-4);

    e.set_offset(4.0);
    assert!(
        (timeline_content_secs(source, &[e]) - 14.0).abs() < 1e-4,
        "素材带右缘 14s 时内容长度必须扩展到 14s"
    );

    // 双轨取较大者；源长本身是下界
    let mut audio = ClipTrackEdit::default();
    audio.set_trim_end(2.0, source);
    let both = [ClipTrackEdit::default(), audio];
    assert!((timeline_content_secs(source, &both) - 10.0).abs() < 1e-4);
    assert!((timeline_content_secs(source, &[e, audio]) - 14.0).abs() < 1e-4);
    // 退化的源长（NaN）不得 panic，结果保持有限
    assert!(timeline_content_secs(f32::NAN, &[e]).is_finite());
}

#[test]
fn test_clip_transport_advance_and_auto_stop() {
    let mut s = VideoClipState::new();
    // 非播放态推进无效
    s.advance_clip_transport(0.5, 10.0);
    assert!(s.clip_position_secs.abs() < f32::EPSILON);

    // 播放推进累加
    s.clip_playing = true;
    s.advance_clip_transport(0.25, 10.0);
    s.advance_clip_transport(0.25, 10.0);
    assert!((s.clip_position_secs - 0.5).abs() < f32::EPSILON);
    assert!(s.clip_playing);

    // 到时间轴内容末尾自动停止并钉在末尾
    s.advance_clip_transport(99.0, 10.0);
    assert!((s.clip_position_secs - 10.0).abs() < f32::EPSILON);
    assert!(!s.clip_playing);

    // 素材带只占 [3,4] 时，播放头仍可走过带外区间（自由游标，不因带而截止）
    let mut t = VideoClipState::new();
    t.clip_playing = true;
    t.advance_clip_transport(3.5, 10.0);
    assert!((t.clip_position_secs - 3.5).abs() < f32::EPSILON);
    assert!(t.clip_playing, "走过素材带不应中断时间轴播放");
}

#[test]
fn test_clip_rewind_and_toggle() {
    let mut s = VideoClipState::new();
    s.clip_playing = true;
    s.set_clip_position(7.0);
    s.clip_rewind();
    assert!((s.clip_position_secs).abs() < f32::EPSILON);
    assert!(!s.clip_playing);
    s.clip_toggle_play();
    assert!(s.clip_playing);
}
