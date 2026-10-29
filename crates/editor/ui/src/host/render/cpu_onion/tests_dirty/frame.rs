//! 布局变化与停用重启用（`tests_dirty.rs` 的子模块）。

use super::{MUTED, WIDTH, document, key};
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
