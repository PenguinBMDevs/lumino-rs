//! 洋葱皮全局开关（`ViewState.onion_visible`）的像素级契约。
//!
//! 用户需求：「点击按钮后显示全部音轨的洋葱皮，关闭就彻底关闭；GPU 模式下
//! 关闭不卸载数据，只不显示」。本文件用**真实 GPU 回读**锁定两件事：
//! 1. 开关关闭 → 非主音轨实例一个片元都不落（主音轨照常显示）；
//! 2. 重新打开 → 同一份实例缓冲（零重传）立刻恢复洋葱皮。
//!
//! 独立文件以保持 `point_tests.rs` < 400 行；渲染与断言辅助复用父模块。

use crate::NoteInstance;

use super::{NOTE_COLOR, covered_rows, encode, expected_row, key_row_covered, render_points};

/// **契约 5**：开关关闭时只画主音轨，实例数据不卸载、重新打开即时恢复。
#[test]
fn test_point_mode_hides_onion_when_disabled() {
    let notes = vec![
        // 主音轨（track_enc 1 = current_track）
        NoteInstance::new(0.0, 90, 120.0, NOTE_COLOR, encode(1, 2)),
        // 洋葱皮轨（track_enc 2，未静音）：开关关闭时必须不可见
        NoteInstance::new(0.0, 70, 120.0, NOTE_COLOR, encode(2, 2)),
    ];

    let hidden_rows = covered_rows(&render_points(&notes, 1, &[], false));
    assert!(
        key_row_covered(&hidden_rows, 90),
        "关闭洋葱皮后主音轨必须照常绘制（key 90 → 期望行 {}）",
        expected_row(90)
    );
    assert!(
        !key_row_covered(&hidden_rows, 70),
        "关闭洋葱皮后非主音轨不得绘制（key 70 → 期望行 {}）",
        expected_row(70)
    );

    // 同一份实例数据（零重传）只翻开关：洋葱皮必须立刻回来
    let shown_rows = covered_rows(&render_points(&notes, 1, &[], true));
    assert!(
        key_row_covered(&shown_rows, 70),
        "重新打开洋葱皮后非主音轨必须恢复显示（数据未重传，key 70 → 期望行 {}）",
        expected_row(70)
    );
}
