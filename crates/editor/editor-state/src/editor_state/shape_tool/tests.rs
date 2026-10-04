//! 形状工具几何与状态测试

use super::*;

#[test]
fn test_rectangle_filled_cells() {
    // 4×4 矩形（key 60..64, tick 0..4 snap=1）：内部 16 格
    let cells = shape_cells(
        ShapeKind::Rectangle,
        (0.0, 60.0, 4.0, 64.0),
        false,
        true,
        1.0,
        1.0,
        1.0,
    );
    assert_eq!(cells.len(), 5 * 5);
    assert!(cells.contains(&(0.0, 60)));
    assert!(cells.contains(&(4.0, 64)));
}

#[test]
fn test_rectangle_outline_cells() {
    // 4×4 矩形轮廓：外圈 = 5*5 - 3*3 = 16 格
    let cells = shape_cells(
        ShapeKind::Rectangle,
        (0.0, 60.0, 4.0, 64.0),
        false,
        false,
        1.0,
        1.0,
        1.0,
    );
    assert_eq!(cells.len(), 5 * 5 - 3 * 3);
}

#[test]
fn test_shift_rectangle_is_square() {
    // 非正方形外接框 + Shift → 约束为正方形（取最小边）
    let cells = shape_cells(
        ShapeKind::Rectangle,
        (0.0, 60.0, 10.0, 64.0),
        true,
        true,
        1.0,
        1.0,
        1.0,
    );
    // 约束后应为 4×4（高度决定边长），故 25 格
    assert_eq!(cells.len(), 5 * 5);
}

#[test]
fn test_circle_center_cells() {
    // 半径 2 的圆（snap=1），中心格必在内
    let cells = shape_cells(
        ShapeKind::Circle,
        (-2.0, 60.0, 2.0, 64.0),
        false,
        true,
        1.0,
        1.0,
        1.0,
    );
    // 中心 (0,62) 必包含
    assert!(cells.contains(&(0.0, 62)));
    // 四角 (±2, 60/64) 在圆周外（椭圆方程 = (2/2)^2+(2/2)^2 = 2 > 1）
    assert!(!cells.contains(&(2.0, 60)));
}

#[test]
fn test_triangle_cells() {
    // 底边 0..4，顶点在 (2,64) 的三角形，底边中点 (2,60) 必在内
    let cells = shape_cells(
        ShapeKind::Triangle,
        (0.0, 60.0, 4.0, 64.0),
        false,
        true,
        1.0,
        1.0,
        1.0,
    );
    assert!(cells.contains(&(2.0, 60)));
    // 顶点附近 (2,64) 必在内
    assert!(cells.contains(&(2.0, 64)));
}

#[test]
fn test_shift_circle_is_regular() {
    // 外接框非正方形，Shift → 正圆（rx=ry=min=2）
    let cells = shape_cells(
        ShapeKind::Circle,
        (0.0, 60.0, 10.0, 64.0),
        true,
        true,
        1.0,
        1.0,
        1.0,
    );
    // 正圆半径 2，中心 (5,62)，中心格在内
    assert!(cells.contains(&(5.0, 62)));
    // 外接框右上 (10,64) 距中心 (5,2) → 椭圆方程 = (5/2)^2+(2/2)^2 = 7.25 > 1
    assert!(!cells.contains(&(10.0, 64)));
}

// ── 描边折线（shape_outline_path） ─────────────────────────

/// 闭合环判定靠 `pts[0] == pts[last]` 逐位相等：圆形的末点必须**直接复用首点**，
/// 而不是再算一次 `cos(TAU)`（f32 下未必逐位相等，差一位就退化成开放路径）。
#[test]
fn test_circle_outline_path_is_closed() {
    let path = shape_outline_path(ShapeKind::Circle, (0.0, 60.0, 4.0, 64.0), false, 1.0, 1.0);
    assert_eq!(
        path.len(),
        CIRCLE_OUTLINE_SEGMENTS + 1,
        "采样段数 + 一个闭合点"
    );
    assert_eq!(
        path.first(),
        path.last(),
        "圆描边折线必须首尾逐位相同（闭合环）"
    );
}

#[test]
fn test_rectangle_and_triangle_outline_paths_are_closed() {
    let rect = shape_outline_path(
        ShapeKind::Rectangle,
        (0.0, 60.0, 4.0, 64.0),
        false,
        1.0,
        1.0,
    );
    // 矩形：4 顶点 + 闭合点（顶点序与 shape_vertices 一致）
    assert_eq!(
        rect,
        vec![
            (0.0, 60.0),
            (4.0, 60.0),
            (4.0, 64.0),
            (0.0, 64.0),
            (0.0, 60.0),
        ]
    );

    let tri = shape_outline_path(ShapeKind::Triangle, (0.0, 60.0, 4.0, 64.0), false, 1.0, 1.0);
    // 三角形：3 顶点 + 闭合点，顶点 = 底边两点 + 顶边中点
    assert_eq!(
        tri,
        vec![(0.0, 64.0), (4.0, 64.0), (2.0, 60.0), (0.0, 64.0)]
    );
}

/// 描边折线同样吃屏幕空间的正图形约束（与预览、命中测试同一口径）：
/// tick 与 key 的像素尺度悬殊时，约束必须交给 `effective_rect`，
/// 否则「看到的圆」与「生成的描边」分叉。
#[test]
fn test_circle_outline_path_respects_screen_constraint() {
    // px_per_tick = 0.1、px_per_key = 10：宽 10 tick 只有 1px，高 4 key 有 40px
    // → 屏幕正圆取半径 0.5px = 5 tick，故 tick 半径 5、key 半径 0.05
    let path = shape_outline_path(ShapeKind::Circle, (0.0, 60.0, 10.0, 64.0), true, 0.1, 10.0);
    let ticks: Vec<f32> = path.iter().map(|p| p.0).collect();
    let keys: Vec<f32> = path.iter().map(|p| p.1).collect();
    let tick_lo = ticks.iter().copied().fold(f32::INFINITY, f32::min);
    let tick_hi = ticks.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let key_lo = keys.iter().copied().fold(f32::INFINITY, f32::min);
    let key_hi = keys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let center = ((0.0 + 10.0) / 2.0, (60.0 + 64.0) / 2.0);
    assert!(
        (tick_hi - tick_lo - 10.0).abs() < 1e-3,
        "屏幕正圆 tick 直径应为 10（实际 {}）",
        tick_hi - tick_lo
    );
    assert!(
        (key_hi - key_lo - 0.1).abs() < 1e-3,
        "屏幕正圆 key 直径应为 0.1（实际 {}）",
        key_hi - key_lo
    );
    assert!((tick_lo + tick_hi) / 2.0 - center.0 < 1e-3);
    assert!((key_lo + key_hi) / 2.0 - center.1 < 1e-3);
}
