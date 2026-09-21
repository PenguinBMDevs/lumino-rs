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
