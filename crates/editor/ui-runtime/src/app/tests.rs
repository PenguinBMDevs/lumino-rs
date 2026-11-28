//! 原生运行时状态单测。

use super::state::note_border_width;

/// `track_enc` 必须写进 border_width 高 16 位：cull shader 用它区分主音轨，
/// 漏写会让画布上的所有音符被裁掉（真机表现为「画了但看不到」）。
#[test]
fn note_border_width_encodes_track_enc() {
    assert_eq!(note_border_width(0) >> 16, 1);
    assert_eq!(note_border_width(1) >> 16, 2);
    assert_eq!(note_border_width(0) & 0xffff, 1);
    assert_eq!(note_border_width(1) & 0xffff, 1);
}
