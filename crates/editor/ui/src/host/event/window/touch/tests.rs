//! 触摸手势层的纯函数单元测试
//!
//! 只覆盖不依赖 `Host`（窗口/渲染上下文）的判定数学；事件状态机本身的
//! 端到端行为需真机或 `Host` 级测试，见交付报告的验证清单。

use super::*;

/// 低于阈值的距离变化不产生缩放（防手指抖动误缩放）
#[test]
fn pinch_ratio_ignores_jitter_below_threshold() {
    assert_eq!(pinch_ratio(100.0, 100.5), None);
    assert_eq!(pinch_ratio(100.0, 99.0), None);
}

/// 越过阈值的距离变化产生带符号的缩放增量
#[test]
fn pinch_ratio_reports_zoom_past_threshold() {
    let outward = pinch_ratio(100.0, 110.0).expect("张开 10% 应产生缩放增量");
    assert!(
        (outward - 0.1).abs() < 1e-4,
        "张开应为正增量，实际 {outward}"
    );

    let inward = pinch_ratio(100.0, 90.0).expect("收拢 10% 应产生缩放增量");
    assert!((inward + 0.1).abs() < 1e-4, "收拢应为负增量，实际 {inward}");
}

/// 单次增量夹在 ±0.15，避免快速张合导致画面跳变
#[test]
fn pinch_ratio_clamps_large_jump() {
    let ratio = pinch_ratio(100.0, 400.0).expect("大幅张开应产生缩放增量");
    assert!(
        (ratio - PINCH_MAX_DELTA).abs() < 1e-4,
        "应夹到上限，实际 {ratio}"
    );
}

/// 锚点非法（0 或负）时不做除法，直接返回 None
#[test]
fn pinch_ratio_rejects_invalid_anchor() {
    assert_eq!(pinch_ratio(0.0, 50.0), None);
    assert_eq!(pinch_ratio(-10.0, 50.0), None);
    assert_eq!(pinch_ratio(50.0, 0.0), None);
}

/// 慢速捏合必须能累加生效：逐帧只变 1% 时，两帧累计 2% 就要出一次增量
///
/// 这是旧实现（逐帧比较 + 每帧前移 prev）的真机缺陷：慢速捏合永远不动。
#[test]
fn pinch_ratio_accumulates_slow_motion() {
    // 锚点 100，单帧只到 101（1%<2% 阈值）→ 不缩放
    assert_eq!(pinch_ratio(100.0, 101.0), None);
    // 锚点不动（仍是 100），第二帧到 103（3%>2%）→ 必须出增量
    let ratio = pinch_ratio(100.0, 103.0).expect("慢速捏合累计越过阈值后应产生缩放");
    assert!((ratio - 0.03).abs() < 1e-4, "增量应为 3%，实际 {ratio}");
}

/// 死区内的中点位移不产生滚动
#[test]
fn pan_increment_ignores_deadzone() {
    let base = Point::new(100.0, 100.0);
    assert_eq!(pan_increment(base, Point::new(100.5, 100.5)), None);
}

/// 越过死区的中点位移按手指方向给出像素增量（手指下移 → dy 为正）
#[test]
fn pan_increment_follows_finger() {
    let base = Point::new(100.0, 100.0);
    let (dx, dy) = pan_increment(base, Point::new(130.0, 160.0)).expect("应产生滚动增量");
    assert!((dx - 30.0).abs() < 1e-4, "水平增量应为 30，实际 {dx}");
    assert!((dy - 60.0).abs() < 1e-4, "垂直增量应为 60，实际 {dy}");
}

/// 长按抖动判定用的是平方距离，边界必须严格对齐 8 逻辑像素
#[test]
fn squared_distance_matches_long_press_slop() {
    let origin = Point::new(10.0, 10.0);
    let slop_squared = LONG_PRESS_SLOP * LONG_PRESS_SLOP;

    // 恰好 8 像素：不大于阈值，仍算长按
    assert!(
        squared_distance(origin, Point::new(18.0, 10.0)) <= slop_squared,
        "恰好 8 像素应保持长按"
    );
    // 斜向 5/6 像素位移（距离 ≈7.8 < 8）：仍在长按范围内
    assert!(
        squared_distance(origin, Point::new(15.0, 16.0)) <= slop_squared,
        "8 像素内的任意方向都应保持长按"
    );
    // 9 像素：超过阈值，取消长按
    assert!(
        squared_distance(origin, Point::new(19.0, 10.0)) > slop_squared,
        "超过 8 像素应取消长按"
    );
}
