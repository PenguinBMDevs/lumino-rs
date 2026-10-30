//! 布局变化与停用重启用（`tests_dirty.rs` 的子模块）。

use super::{MUTED, WIDTH, alpha, document, key, note_row};
use crate::host::render::cpu_onion::CpuOnionState;

/// 布局变化（相机滚动）仍整帧重绘且不重建索引（版本递增）。
#[test]
fn layout_change_redraws_full_frame() {
    let document = document();
    let mut state = CpuOnionState::default();
    let first = state
        .update(key(1, 0), &document, &MUTED, None, None)
        .frame
        .expect("首帧应生成");

    let mut scrolled = key(1, 0);
    scrolled.geometry.width = WIDTH as u32;
    scrolled.geometry.camera.scroll_x = 100.0;
    let output = state.update(scrolled, &document, &MUTED, None, None);
    assert!(output.rebuilt, "滚动必须重绘");
    assert_ne!(
        output.frame.expect("应生成帧").version,
        first.version,
        "版本必须递增"
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
