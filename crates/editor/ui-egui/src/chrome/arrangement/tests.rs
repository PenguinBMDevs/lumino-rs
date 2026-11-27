//! 走带标尺的坐标换算单测。
//!
//! 从 `arrangement.rs` 拆出（生产代码 + 测试合计控制在 600 行以内）。

use super::*;

fn model() -> ArrangementRulerModel {
    ArrangementRulerModel {
        scroll_x: 200.0,
        zoom_x: 0.5,
        ppq: 960,
        left_inset: 0.0,
        top_inset: 0.0,
        time_signatures: vec![(0, 4, 4)],
        total_ticks: 100_000,
    }
}

fn rect() -> Rect {
    Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(800.0, RULER_HEIGHT))
}

#[test]
fn tick_at_subtracts_scroll_and_clamps() {
    let m = model();
    let r = rect();
    // 指针在标尺左缘：local 0 → (0 + 200) / 0.5 = 400 tick。
    assert!((tick_at(r, &m, 100.0) - 400.0).abs() < 1e-6);
    // 右移 100 像素：local 100 → (100 + 200) / 0.5 = 600 tick。
    assert!((tick_at(r, &m, 200.0) - 600.0).abs() < 1e-6);
    // 左越界钳 0，右越界钳 total_ticks。
    assert_eq!(tick_at(r, &m, -50.0), 400.0);
    assert_eq!(tick_at(r, &m, 100_000.0), 100_000.0);
}

#[test]
fn tick_to_x_inverts_tick_at() {
    let m = model();
    let r = rect();
    let x = tick_to_x(r, &m, 1000.0);
    assert!((tick_at(r, &m, x) - 1000.0).abs() < 1e-3);
}
