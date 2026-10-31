//! 布局变化、增量滚动与停用重启用（`tests_dirty.rs` 的子模块）。

use super::{MUTED, alpha, document, key, note_row, reference_pixels_at};
use crate::host::render::cpu_onion::CpuOnionState;

/// 非横向滚动的布局变化（纵向滚动）仍整帧重绘且不重建索引（版本递增）。
#[test]
fn layout_change_redraws_full_frame() {
    let document = document();
    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let mut scrolled = key(1, 0);
    scrolled.geometry.camera.scroll_y = 100.0;
    let output = state.update(scrolled, &document, &MUTED, None, None);
    assert!(output.rebuilt, "纵向滚动必须重绘");
    assert_ne!(
        output.frame.expect("应生成帧").version,
        first.version,
        "版本必须递增"
    );
}

/// 纯水平滚动：平移 + 窄条必须与「同滚动位置冷启动整帧」逐像素一致。
#[test]
fn horizontal_scroll_shifts_frame_and_matches_full_render() {
    let document = document();
    let mut state = CpuOnionState::default();
    state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let mut scrolled = key(1, 0);
    scrolled.geometry.camera.scroll_x = 10.0;
    let output = state.update(scrolled, &document, &MUTED, None, None);
    assert!(output.rebuilt, "水平滚动必须重绘新露出区域");
    assert_eq!(
        output.dirty, None,
        "纹理整体位移，必须全量上传（不能只传窄条）"
    );
    let frame = output.frame.expect("应生成帧");
    assert_eq!(
        frame.pixels,
        reference_pixels_at(&document, 0, 10.0),
        "平移+窄条不得与整帧渲染有任何像素差"
    );
    assert_eq!(
        state.visible_notes(),
        0,
        "窄条 [630,640) 内没有音符：visible_notes=0 证明走的是增量窄条而非整帧"
    );
}

/// 反向（向左）滚动同样逐像素一致。
#[test]
fn negative_horizontal_scroll_matches_full_render() {
    let document = document();
    let mut state = CpuOnionState::default();
    let mut start = key(1, 0);
    start.geometry.camera.scroll_x = 100.0;
    state
        .update(start, &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let mut back = start;
    back.geometry.camera.scroll_x = 90.0;
    let output = state.update(back, &document, &MUTED, None, None);
    assert!(output.rebuilt);
    assert_eq!(
        output.frame.expect("应生成帧").pixels,
        reference_pixels_at(&document, 0, 90.0),
        "反向滚动也必须与整帧渲染逐像素一致"
    );
}

/// 大跨度滚动（超过帧宽 60%）不接受窄条增量，退回整帧。
#[test]
fn large_scroll_jump_matches_full_render() {
    let document = document();
    let mut state = CpuOnionState::default();
    state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let mut jump = key(1, 0);
    jump.geometry.camera.scroll_x = 500.0;
    let output = state.update(jump, &document, &MUTED, None, None);
    assert!(output.rebuilt);
    assert_eq!(
        output.frame.expect("应生成帧").pixels,
        reference_pixels_at(&document, 0, 500.0),
        "大跨度必须整帧且像素正确"
    );
}

/// 亚像素位移（吸附未生效/防御路径）不得走整数平移，退回整帧。
#[test]
fn fractional_scroll_falls_back_to_full_render() {
    let document = document();
    let mut state = CpuOnionState::default();
    state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let mut fractional = key(1, 0);
    fractional.geometry.camera.scroll_x = 0.4;
    let output = state.update(fractional, &document, &MUTED, None, None);
    assert!(output.rebuilt);
    assert_eq!(
        output.frame.expect("应生成帧").pixels,
        reference_pixels_at(&document, 0, 0.4),
        "亚像素滚动必须整帧，不得拿旧帧猜"
    );
}

/// 停用后重新启用：全量重建，但**版本必须单调递增**（不得回退到 1）。
///
/// `version` 是 gfx 侧判断「纹理内容是否同一份」的唯一标识：回退会让新会话首帧
/// 被误判为 `Reuse`，纹理仍是上一会话的旧图（空白工程里「CPU → GPU → 编辑 →
/// CPU」完全可达）。同时首帧必须是整帧重绘 → 无脏区 → gfx 走全量上传。
#[test]
fn deactivate_keeps_version_monotonic() {
    let document = document();
    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");
    assert_eq!(first.version, 1, "冷启动首帧版本从 1 开始");

    state.deactivate();
    assert!(state.frame().is_none(), "停用后不得保留帧");

    let rebuilt = state.update(key(1, 0), &document, &MUTED, None, None);
    assert!(rebuilt.rebuilt, "重新启用必须整帧重绘");
    assert_eq!(rebuilt.dirty, None, "重新启用首帧是整帧重绘，不得上报脏区");
    let frame = rebuilt.frame.expect("应可重建");
    assert_eq!(
        frame.version, 2,
        "版本必须单调递增（停用不得归零，否则 gfx 会把新内容误判为 Reuse）"
    );
    assert_eq!(frame.dirty_rect, None, "整帧重绘不得携带脏区");
}

/// 缩放（X/Y）必须让冻结帧失效，并按新相机重画到正确像素行/列。
///
/// 只改 `scroll` 的布局测试无法覆盖这条：缩放同时改变 `zoom_x` / `zoom_y`，
/// 旧帧若被复用，音符会停在旧像素位置，表现为「缩放后音符与网格错位」。
/// 从总览缩放到 Exact：必须换掉整帧热力图，回到真实音符像素。
///
/// 这是 LOD 与纹理刷新的交界：低缩放走 Overview，高缩放第一帧必须
/// `mode_changed → Full`，且新帧不能复用旧热力图纹理。
#[test]
fn one_tick_notes_stay_exact_at_high_zoom() {
    let mut document = lumino_midi_model::MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, lumino_midi_model::NoteEvent::new(10, 11, 121, 100, 0));
    let mut state = CpuOnionState::default();
    let mut zoomed = key(1, 0);
    zoomed.geometry.camera.zoom_x = 1.0;
    zoomed.geometry.camera.zoom_y = 20.0;
    let frame = state
        .update(zoomed, &document, &[false; 2], None, None)
        .frame
        .expect("高缩放 Exact 帧应生成");

    assert!(
        alpha(&frame, 10, note_row(121)) > 0,
        "1px 宽短音符在 1px/tick 下必须按 Exact 画出"
    );
    assert_eq!(
        alpha(&frame, 40, note_row(121)),
        0,
        "不得退化成覆盖整列的密度热力图"
    );
}

#[test]
fn overview_to_exact_with_many_tracks_matches_full_render() {
    let mut document = lumino_midi_model::MidiDocument::empty_with_tracks(8, 480);
    for track in 0..8usize {
        for index in 0..300u32 {
            let start = index * 10 + track as u32;
            document.insert_note(
                track,
                lumino_midi_model::NoteEvent::new(
                    start,
                    start + 10,
                    40 + track as u8 * 5,
                    100,
                    track as u8,
                ),
            );
        }
    }
    let mut state = CpuOnionState::default();
    let mut overview_key = key(1, 0);
    overview_key.geometry.camera.zoom_x = 0.01;
    state
        .update(overview_key, &document, &[false; 8], None, None)
        .frame
        .expect("总览帧应生成");

    let mut exact_key = key(1, 0);
    exact_key.geometry.camera.zoom_x = 1.0;
    let exact = state
        .update(exact_key, &document, &[false; 8], None, None)
        .frame
        .expect("Exact 帧应生成");
    let mut reference = CpuOnionState::default();
    let expected = reference
        .update(exact_key, &document, &[false; 8], None, None)
        .frame
        .expect("参考帧应生成");
    assert_eq!(
        exact.pixels, expected.pixels,
        "多轨/多音符下退出总览仍必须与冷启动 Exact 一致"
    );
}

#[test]
fn overview_to_exact_replaces_heatmap_with_notes() {
    let document = document();
    let mut state = CpuOnionState::default();
    let mut overview_key = key(1, 0);
    overview_key.geometry.camera.zoom_x = 0.01;
    let overview = state
        .update(overview_key, &document, &MUTED, None, None)
        .frame
        .expect("总览帧应生成");

    let mut exact_key = key(1, 0);
    exact_key.geometry.camera.zoom_x = 1.0;
    let output = state.update(exact_key, &document, &MUTED, None, None);
    assert!(output.rebuilt, "退出总览必须整帧重绘");
    let frame = output.frame.expect("Exact 帧应生成");

    let mut reference = CpuOnionState::default();
    let expected = reference
        .update(exact_key, &document, &MUTED, None, None)
        .frame
        .expect("参考 Exact 帧应生成");
    assert_eq!(
        frame.pixels, expected.pixels,
        "退出总览后必须与冷启动 Exact 逐像素一致，不得残留热力图"
    );
    assert!(
        !std::sync::Arc::ptr_eq(&overview.pixels, &frame.pixels),
        "跨档必须换新像素缓冲，不能复用热力图纹理"
    );
    assert_ne!(frame.version, overview.version, "换档必须推进纹理版本");
    assert!(
        alpha(&frame, 50, note_row(121)) > 0,
        "音符必须画回 key=121 的 Exact 行"
    );
}

#[test]
fn zoom_redraws_frame_at_new_camera_positions() {
    let document = document();
    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    // 轨 1 是洋葱轨：tick=10、key=121；初始 20px/key，中心行 130。
    assert!(
        alpha(&first, 60, note_row(121)) > 0,
        "初始帧应画在 key=121 的 20px 行"
    );

    // Y 缩放：20 → 10。key=121 的中心行从 130 移到 65；旧行必须被清掉。
    let mut zoomed_y = key(1, 0);
    zoomed_y.geometry.camera.zoom_y = 10.0;
    let output = state.update(zoomed_y, &document, &MUTED, None, None);
    assert!(output.rebuilt, "Y 缩放必须重绘冻结帧");
    let frame_y = output.frame.expect("Y 缩放后应生成帧");
    assert!(
        alpha(&frame_y, 60, 65) > 0,
        "key=121 在 10px/key 下应画到中心行 65"
    );
    assert_eq!(
        alpha(&frame_y, 60, note_row(121)),
        0,
        "旧相机位置不得残留音符像素"
    );

    // X 缩放：1 → 2。tick=10 的左边界从 x=10 移到 x=20，x=15 必须落到音符外。
    let mut zoomed_x = zoomed_y;
    zoomed_x.geometry.camera.zoom_x = 2.0;
    let output = state.update(zoomed_x, &document, &MUTED, None, None);
    assert!(output.rebuilt, "X 缩放必须重绘冻结帧");
    let frame_x = output.frame.expect("X 缩放后应生成帧");
    assert_eq!(
        alpha(&frame_x, 15, 65),
        0,
        "tick=10 在 2px/tick 下左边界是 x=20，x=15 不得再有音符"
    );
    assert!(
        alpha(&frame_x, 25, 65) > 0,
        "tick=10 在 2px/tick 下应覆盖 x=25"
    );
}
