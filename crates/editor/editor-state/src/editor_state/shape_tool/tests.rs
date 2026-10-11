//! 形状工具几何与状态测试

use super::*;

/// 测试用几何参数
///
/// `apex_high` = 三角形顶点是否在 key 大的一侧（由拖拽方向决定，
/// 见 [`ShapeSpec::apex_high`]）：向下拉 = `true` = 屏幕正立、向上拉 = `false` = 屏幕倒立。
fn spec(
    kind: ShapeKind,
    rect: (f32, f32, f32, f32),
    shift_constrained: bool,
    apex_high: bool,
) -> ShapeSpec {
    ShapeSpec {
        kind,
        rect,
        shift_constrained,
        apex_high,
    }
}

#[test]
fn test_rectangle_filled_cells() {
    // 4×4 矩形（key 60..64, tick 0..4 snap=1）：内部 16 格
    let s = spec(ShapeKind::Rectangle, (0.0, 60.0, 4.0, 64.0), false, true);
    let cells = shape_cells(s, true, 1.0, 1.0, 1.0);
    assert_eq!(cells.len(), 5 * 5);
    assert!(cells.contains(&(0.0, 60)));
    assert!(cells.contains(&(4.0, 64)));
}

#[test]
fn test_rectangle_outline_cells() {
    // 4×4 矩形轮廓：外圈 = 5*5 - 3*3 = 16 格
    let s = spec(ShapeKind::Rectangle, (0.0, 60.0, 4.0, 64.0), false, true);
    let cells = shape_cells(s, false, 1.0, 1.0, 1.0);
    assert_eq!(cells.len(), 5 * 5 - 3 * 3);
}

#[test]
fn test_shift_rectangle_is_square() {
    // 非正方形外接框 + Shift → 约束为正方形（取最小边）
    let s = spec(ShapeKind::Rectangle, (0.0, 60.0, 10.0, 64.0), true, true);
    let cells = shape_cells(s, true, 1.0, 1.0, 1.0);
    // 约束后应为 4×4（高度决定边长），故 25 格
    assert_eq!(cells.len(), 5 * 5);
}

#[test]
fn test_circle_center_cells() {
    // 半径 2 的圆（snap=1），中心格必在内
    let s = spec(ShapeKind::Circle, (-2.0, 60.0, 2.0, 64.0), false, true);
    let cells = shape_cells(s, true, 1.0, 1.0, 1.0);
    // 中心 (0,62) 必包含
    assert!(cells.contains(&(0.0, 62)));
    // 四角 (±2, 60/64) 在圆周外（椭圆方程 = (2/2)^2+(2/2)^2 = 2 > 1）
    assert!(!cells.contains(&(2.0, 60)));
}

#[test]
fn test_shift_circle_is_regular() {
    // 外接框非正方形，Shift → 正圆（rx=ry=min=2）
    let s = spec(ShapeKind::Circle, (0.0, 60.0, 10.0, 64.0), true, true);
    let cells = shape_cells(s, true, 1.0, 1.0, 1.0);
    // 正圆半径 2，中心 (5,62)，中心格在内
    assert!(cells.contains(&(5.0, 62)));
    // 外接框右上 (10,64) 距中心 (5,2) → 椭圆方程 = (5/2)^2+(2/2)^2 = 7.25 > 1
    assert!(!cells.contains(&(10.0, 64)));
}

// ── 三角形朝向（顶点朝拖拽起点侧） ─────────────────────────

/// 正立（向下拉）：顶点在 key **大**的一侧 = 屏幕上方，底边在 key 60 贯通 0..4
#[test]
fn test_upright_triangle_cells() {
    let s = spec(ShapeKind::Triangle, (0.0, 60.0, 4.0, 64.0), false, true);
    let cells = shape_cells(s, true, 1.0, 1.0, 1.0);
    assert!(cells.contains(&(2.0, 64)), "顶点格必在内");
    assert!(
        cells.contains(&(0.0, 60)) && cells.contains(&(4.0, 60)),
        "底边两端格在内（底边最宽）"
    );
    assert!(
        !cells.contains(&(0.0, 64)) && !cells.contains(&(1.0, 64)) && !cells.contains(&(3.0, 64)),
        "顶点行只有顶点列在内"
    );
}

/// 倒立（向上拉）：同一外接框、`apex_high = false` → 与正立逐行镜像
#[test]
fn test_inverted_triangle_cells() {
    let s = spec(ShapeKind::Triangle, (0.0, 60.0, 4.0, 64.0), false, false);
    let cells = shape_cells(s, true, 1.0, 1.0, 1.0);
    assert!(cells.contains(&(2.0, 60)), "顶点格在 key 60");
    assert!(
        !cells.contains(&(0.0, 60)) && !cells.contains(&(1.0, 60)),
        "顶点行只有顶点列在内"
    );
    assert!(
        cells.contains(&(0.0, 64)) && cells.contains(&(4.0, 64)),
        "底边在 key 64 贯通"
    );
}

/// 朝向由**拖拽方向**决定：向下拉 = 正立、向上拉 = 倒立，外接框两者相同
#[test]
fn test_triangle_apex_follows_drag_direction() {
    let mut st = ShapeToolState::default();
    st.set_shape_kind(ShapeKind::Triangle);
    // 向下拉：起点 key 64 → 当前 key 60
    st.begin_drag((0.0, 64.0));
    st.update_drag((4.0, 60.0));
    let down = st.end_drag(1.0, false).expect("向下拉应有图形");
    assert!(down.apex_high, "向下拉 ⇒ 顶点在高音高侧（屏幕正立）");
    assert_eq!(down.rect, (0.0, 60.0, 4.0, 64.0), "外接框仍规范化");

    // 向上拉：起点 key 60 → 当前 key 64
    let mut st = ShapeToolState::default();
    st.set_shape_kind(ShapeKind::Triangle);
    st.begin_drag((0.0, 60.0));
    st.update_drag((4.0, 64.0));
    let up = st.end_drag(1.0, false).expect("向上拉应有图形");
    assert!(!up.apex_high, "向上拉 ⇒ 顶点在低音高侧（屏幕倒立）");
    assert_eq!(
        up.rect, down.rect,
        "两种方向的规范化外接框相同，方向由 apex_high 单独承载"
    );
}

/// 拖拽过程中越过起点 ⇒ 预览朝向**实时翻转**（每帧重算，不是松手才定）
#[test]
fn test_preview_apex_flips_when_crossing_start() {
    let mut st = ShapeToolState::default();
    st.set_shape_kind(ShapeKind::Triangle);
    st.begin_drag((0.0, 60.0));
    st.update_drag((4.0, 56.0)); // 向下拉
    assert!(
        st.preview_rect(false).expect("拖拽中").0.apex_high,
        "向下拉 ⇒ 正立"
    );
    st.update_drag((4.0, 64.0)); // 越过起点拉到上方
    assert!(
        !st.preview_rect(false).expect("拖拽中").0.apex_high,
        "越过起点 ⇒ 翻转为倒立"
    );
    st.update_drag((4.0, 58.0)); // 再拉回下方
    assert!(
        st.preview_rect(false).expect("拖拽中").0.apex_high,
        "再拉回下方 ⇒ 翻回正立"
    );
}

/// 同一外接框下正立 / 倒立互为逐点镜像（描边折线 + 顶点序）
#[test]
fn test_triangle_orientation_mirrors_geometry() {
    let rect = (0.0, 60.0, 4.0, 64.0);
    let upright = spec(ShapeKind::Triangle, rect, false, true);
    let inverted = spec(ShapeKind::Triangle, rect, false, false);
    let path = |s| shape_outline_path(s, 1.0, 1.0);
    assert_eq!(
        path(upright),
        vec![(0.0, 60.0), (4.0, 60.0), (2.0, 64.0), (0.0, 60.0)],
        "正立：顶点 (2,64) + 闭环"
    );
    assert_eq!(
        path(inverted),
        vec![(0.0, 64.0), (4.0, 64.0), (2.0, 60.0), (0.0, 64.0)],
        "倒立：顶点 (2,60) + 闭环"
    );
    assert_eq!(
        shape_vertices(upright, 1.0, 1.0).expect("三角形有顶点"),
        vec![(0.0, 60.0), (4.0, 60.0), (2.0, 64.0)]
    );
    assert_eq!(
        shape_vertices(inverted, 1.0, 1.0).expect("三角形有顶点"),
        vec![(0.0, 64.0), (4.0, 64.0), (2.0, 60.0)]
    );
}

/// 朝向对**命中判定**同样生效：顶点行只有顶点列命中，两种朝向彼此镜像
#[test]
fn test_point_in_shape_respects_triangle_orientation() {
    let rect = (0.0, 60.0, 4.0, 64.0);
    let upright = spec(ShapeKind::Triangle, rect, false, true);
    let inverted = spec(ShapeKind::Triangle, rect, false, false);
    // 顶点行（key 64）：正立只有顶点列命中，倒立整行是底边
    assert!(point_in_shape(upright, 1.0, 1.0, 2.0, 64.0));
    assert!(!point_in_shape(upright, 1.0, 1.0, 0.0, 64.0));
    assert!(point_in_shape(inverted, 1.0, 1.0, 0.0, 64.0));
    // 另一端的行（key 60）正好相反
    assert!(point_in_shape(upright, 1.0, 1.0, 0.0, 60.0));
    assert!(!point_in_shape(inverted, 1.0, 1.0, 0.0, 60.0));
    assert!(point_in_shape(inverted, 1.0, 1.0, 2.0, 60.0));
}

// ── 描边折线（shape_outline_path） ─────────────────────────

/// 闭合环判定靠 `pts[0] == pts[last]` 逐位相等：圆形的末点必须**直接复用首点**，
/// 而不是再算一次 `cos(TAU)`（f32 下未必逐位相等，差一位就退化成开放路径）。
#[test]
fn test_circle_outline_path_is_closed() {
    let s = spec(ShapeKind::Circle, (0.0, 60.0, 4.0, 64.0), false, true);
    let path = shape_outline_path(s, 1.0, 1.0);
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
fn test_rectangle_outline_path_is_closed() {
    let s = spec(ShapeKind::Rectangle, (0.0, 60.0, 4.0, 64.0), false, true);
    // 矩形：4 顶点 + 闭合点（顶点序与 shape_vertices 一致）
    assert_eq!(
        shape_outline_path(s, 1.0, 1.0),
        vec![
            (0.0, 60.0),
            (4.0, 60.0),
            (4.0, 64.0),
            (0.0, 64.0),
            (0.0, 60.0),
        ]
    );
}

/// Shift 约束的等边三角形：顶点朝拖拽起点侧，且单轴拖拽不得退化成一条发丝
///
/// 规范化后的外接框在「纯水平/纯竖直拖拽」时有一轴位移为 0：老实现用
/// `dy.signum()`（= 0）乘高度，把三角形压成零高；底边宽为 0 时也会压成零宽。
#[test]
fn test_shift_equilateral_triangle_follows_direction_and_does_not_degenerate() {
    let h = 10.0 * 3.0_f32.sqrt() / 2.0; // 底宽 10 → 等边高
    // 向下拉 + 纯水平拖拽（高 0）：底边留在 key 60，顶点向上延伸 h
    let up = effective_rect(
        spec(ShapeKind::Triangle, (0.0, 60.0, 10.0, 60.0), true, true),
        1.0,
        1.0,
    );
    assert_eq!((up.0, up.1, up.2), (0.0, 60.0, 10.0), "底边位置与宽度不变");
    assert!(
        (up.3 - up.1 - h).abs() < 1e-3,
        "顶点应向上（key 大）延伸 h（实际 {}）",
        up.3 - up.1
    );

    // 向上拉：底边留在当前点侧（key 64），顶点向下延伸 h
    let down = effective_rect(
        spec(ShapeKind::Triangle, (0.0, 60.0, 10.0, 64.0), true, false),
        1.0,
        1.0,
    );
    assert_eq!(
        (down.0, down.2, down.3),
        (0.0, 10.0, 64.0),
        "底边留在拖拽当前点侧"
    );
    assert!(
        (down.3 - down.1 - h).abs() < 1e-3,
        "顶点应向下（key 小）延伸 h（实际 {}）",
        down.3 - down.1
    );

    // 纯竖直拖拽（底边宽 0）：改由高度反推底宽（h × 2 / √3），仍不退化
    let vertical = effective_rect(
        spec(ShapeKind::Triangle, (5.0, 60.0, 5.0, 70.0), true, false),
        1.0,
        1.0,
    );
    assert_eq!(
        (vertical.0, vertical.3),
        (5.0, 70.0),
        "底边留在拖拽当前点的 key 边、tick 锚点不变"
    );
    assert!(
        (vertical.3 - vertical.1 - 10.0).abs() < 1e-3,
        "高度应等于反推底宽换算回的高度（实际 {}）",
        vertical.3 - vertical.1
    );
    assert!(
        (vertical.2 - vertical.0 - 10.0 * 2.0 / 3.0_f32.sqrt()).abs() < 1e-3,
        "底边宽应为 h × 2 / √3（实际 {}）",
        vertical.2 - vertical.0
    );
}

/// 同类：矩形 / 圆的 Shift 约束同样不能在单轴拖拽下退化成发丝 / 点
#[test]
fn test_shift_square_and_circle_do_not_degenerate_on_single_axis_drag() {
    // 纯水平拖拽：正方形边长取有位移的那一轴（tick 方向 10）
    let square = effective_rect(
        spec(ShapeKind::Rectangle, (0.0, 60.0, 10.0, 60.0), true, true),
        1.0,
        1.0,
    );
    assert_eq!(square, (0.0, 60.0, 10.0, 70.0), "边长 10 的正方形");

    // 纯水平拖拽：正圆半径 = 水平半径 5px → tick 半径 5、key 半径 5
    let circle = effective_rect(
        spec(ShapeKind::Circle, (0.0, 60.0, 10.0, 60.0), true, true),
        1.0,
        1.0,
    );
    assert_eq!(circle, (0.0, 55.0, 10.0, 65.0), "半径 5 的正圆");
}

/// 描边折线同样吃屏幕空间的正图形约束（与预览、命中测试同一口径）：
/// tick 与 key 的像素尺度悬殊时，约束必须交给 `effective_rect`，
/// 否则「看到的圆」与「生成的描边」分叉。
#[test]
fn test_circle_outline_path_respects_screen_constraint() {
    // px_per_tick = 0.1、px_per_key = 10：宽 10 tick 只有 1px，高 4 key 有 40px
    // → 屏幕正圆取半径 0.5px = 5 tick，故 tick 半径 5、key 半径 0.05
    let s = spec(ShapeKind::Circle, (0.0, 60.0, 10.0, 64.0), true, true);
    let path = shape_outline_path(s, 0.1, 10.0);
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
