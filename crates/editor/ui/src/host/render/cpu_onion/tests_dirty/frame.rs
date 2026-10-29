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

/// 停用后重新启用：全量重建（版本从 1 重新开始），并释放旧缓存。
#[test]
fn deactivate_resets_state() {
    let document = document();
    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &document, &MUTED, None, None);
    state.deactivate();
    assert!(state.frame().is_none(), "停用后不得保留帧");

    let rebuilt = state.update(key(1, 0), &document, &MUTED, None, None);
    assert!(rebuilt.rebuilt);
    assert_eq!(rebuilt.frame.expect("应可重建").version, 1);
}
