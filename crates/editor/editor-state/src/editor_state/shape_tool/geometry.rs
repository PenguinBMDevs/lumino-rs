//! 形状工具几何计算：外接框规范化、屏幕空间正图形约束、顶点、内外判定与格点生成

use super::ShapeKind;

/// 规范化外接框：保证 lo <= hi（与拖拽方向无关）
pub(super) fn normalize_rect(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    let (x0, x1) = if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) };
    let (y0, y1) = if a.1 <= b.1 { (a.1, b.1) } else { (b.1, a.1) };
    (x0, y0, x1, y1)
}

/// 应用 Shift 约束后的圆外接框（屏幕像素空间正圆：rx_px = ry_px = min）
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数。
/// 在**屏幕空间**取半径像素最小值再换算回逻辑 tick/key，保证屏幕上呈现正圆。
fn screen_circle_rect(
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    px_per_tick: f32,
    px_per_key: f32,
) -> (f32, f32, f32, f32) {
    let mx = (x0 + x1) / 2.0;
    let my = (y0 + y1) / 2.0;
    let rx_px = ((x1 - x0) / 2.0).abs() * px_per_tick;
    let ry_px = ((y1 - y0) / 2.0).abs() * px_per_key;
    let r = rx_px.min(ry_px).max(1e-6);
    let rx = r / px_per_tick;
    let ry = r / px_per_key;
    (mx - rx, my - ry, mx + rx, my + ry)
}

/// 应用 Shift 约束后的矩形外接框（屏幕像素空间正方形）
///
/// 在屏幕空间取 min(宽_px, 高_px) 作为边长，再换算回逻辑 tick/key 尺寸，
/// 这样拉出的矩形在屏幕上才是真正的正方形（不再被压扁）。边长带方向符号，
/// 保留用户拖拽的 X / Y 方向。
fn screen_square_rect(
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    px_per_tick: f32,
    px_per_key: f32,
) -> (f32, f32, f32, f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let side = (dx * px_per_tick)
        .abs()
        .min((dy * px_per_key).abs())
        .max(1e-6);
    let w = side / px_per_tick; // 逻辑 tick 边长（保留 dx 方向）
    let h = side / px_per_key; // 逻辑 key 边长（保留 dy 方向）
    (x0, y0, x0 + dx.signum() * w, y0 + dy.signum() * h)
}

/// 应用 Shift 约束后的三角形外接框（屏幕像素空间等边三角形）
///
/// 以矩形底边（沿 tick 的水平边）宽度为基准，屏幕高度 = 底宽_px × √3 / 2，
/// 再换算回逻辑 key 高度，保证屏幕上呈现真正的等边三角形。
fn screen_equilateral_rect(
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    px_per_tick: f32,
    px_per_key: f32,
) -> (f32, f32, f32, f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let base_px = (dx * px_per_tick).abs();
    let height_px = base_px * (3.0_f32).sqrt() / 2.0;
    let h = (height_px / px_per_key).max(1e-6);
    (x0, y0, x0 + dx, y0 + dy.signum() * h)
}

/// 三角形三顶点（未约束：顶点在 (mid, y0)，底边 (x0..x1, y1)）
fn normal_triangle_verts(rect: (f32, f32, f32, f32)) -> [(f32, f32); 3] {
    let (x0, y0, x1, y1) = rect;
    let mid = (x0 + x1) / 2.0;
    [(x0, y1), (x1, y1), (mid, y0)]
}

/// 形状对外有效外接框：统一应用 Shift 正图形约束（**屏幕像素空间**）
///
/// - 矩形 → 屏幕正方形（min(宽_px, 高_px)）；
/// - 圆 → 屏幕正圆（rx_px = ry_px）；
/// - 三角形 → 屏幕等边三角形（高 = 底宽_px × √3 / 2）。
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数：
/// 逻辑空间里 tick 与 key 的像素尺度悬殊，直接在逻辑空间取 min(宽,高) 会让矩形
/// 在屏幕上被压扁（X 向宽度异常压缩、且因 X 被钳到 Y 而不跟手），故约束必须放到
/// 屏幕空间做。几何判定 / 渲染 / 命中测试均应先经此函数并传入相同尺度，保证三者一致。
pub fn effective_rect(
    kind: ShapeKind,
    rect: (f32, f32, f32, f32),
    shift_constrained: bool,
    px_per_tick: f32,
    px_per_key: f32,
) -> (f32, f32, f32, f32) {
    if !shift_constrained {
        return rect;
    }
    // 防止缩放尺度为 0 时换算出现除零 / NaN
    let px_per_tick = px_per_tick.max(1e-6);
    let px_per_key = px_per_key.max(1e-6);
    match kind {
        ShapeKind::Rectangle => screen_square_rect(rect, px_per_tick, px_per_key),
        ShapeKind::Circle => screen_circle_rect(rect, px_per_tick, px_per_key),
        ShapeKind::Triangle => screen_equilateral_rect(rect, px_per_tick, px_per_key),
    }
}

/// 形状多边形顶点（逻辑坐标），用于矢量渲染
///
/// - 矩形：返回 4 个角；
/// - 三角形：返回 3 个顶点（Shift 约束为等边）；
/// - 圆形：无顶点（用椭圆渲染），返回 `None`。
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数，
/// 用于屏幕空间的正图形约束（见 `effective_rect`）。
pub fn shape_vertices(
    kind: ShapeKind,
    rect: (f32, f32, f32, f32),
    shift_constrained: bool,
    px_per_tick: f32,
    px_per_key: f32,
) -> Option<Vec<(f32, f32)>> {
    let rect = effective_rect(kind, rect, shift_constrained, px_per_tick, px_per_key);
    match kind {
        ShapeKind::Rectangle => {
            let (x0, y0, x1, y1) = rect;
            Some(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
        }
        ShapeKind::Triangle => Some(normal_triangle_verts(rect).to_vec()),
        ShapeKind::Circle => None,
    }
}

/// 判断逻辑坐标点 (cx, cy) 是否落在指定图形内部
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数，
/// 用于屏幕空间的正图形约束（见 `effective_rect`）。
pub fn point_in_shape(
    kind: ShapeKind,
    rect: (f32, f32, f32, f32),
    shift_constrained: bool,
    px_per_tick: f32,
    px_per_key: f32,
    cx: f32,
    cy: f32,
) -> bool {
    let rect = effective_rect(kind, rect, shift_constrained, px_per_tick, px_per_key);
    match kind {
        ShapeKind::Rectangle => {
            let (x0, y0, x1, y1) = rect;
            cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1
        }
        ShapeKind::Circle => {
            let (cx0, cy0, cx1, cy1) = rect;
            let mx = (cx0 + cx1) / 2.0;
            let my = (cy0 + cy1) / 2.0;
            let rx = ((cx1 - cx0) / 2.0).max(1e-6);
            let ry = ((cy1 - cy0) / 2.0).max(1e-6);
            let dx = (cx - mx) / rx;
            let dy = (cy - my) / ry;
            dx * dx + dy * dy <= 1.0
        }
        ShapeKind::Triangle => {
            let verts = normal_triangle_verts(rect);
            point_in_triangle((cx, cy), verts[0], verts[1], verts[2])
        }
    }
}

/// 符号函数（叉积），用于三角形内外判定
fn sign(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    (p.0 - b.0) * (a.1 - b.1) - (a.0 - b.0) * (p.1 - b.1)
}

/// 点在三角形内（同向法）
fn point_in_triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let d1 = sign(p, a, b);
    let d2 = sign(p, b, c);
    let d3 = sign(p, c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !has_neg || !has_pos
}

/// 生成图形覆盖的网格格点（逻辑坐标：tick = snap 倍数、key 整数）
///
/// - `filled = true`：图形内部全部格点；
/// - `filled = false`：仅边界格点（轮廓，用于空心图形）。
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数，
/// 用于屏幕空间的正图形约束（见 `effective_rect`）。
pub fn shape_cells(
    kind: ShapeKind,
    rect: (f32, f32, f32, f32),
    shift_constrained: bool,
    filled: bool,
    snap: f32,
    px_per_tick: f32,
    px_per_key: f32,
) -> Vec<(f32, u16)> {
    let snap = snap.max(1.0);
    let (x0, y0, x1, y1) = rect;
    let xi0 = (x0 / snap).floor() as i64;
    let xi1 = (x1 / snap).ceil() as i64;
    let yi0 = y0.floor() as i64;
    let yi1 = y1.ceil() as i64;
    let mut cells = Vec::new();
    for xi in xi0..=xi1 {
        let cx = xi as f32 * snap;
        for yi in yi0..=yi1 {
            let cy = yi as f32;
            if !point_in_shape(
                kind,
                rect,
                shift_constrained,
                px_per_tick,
                px_per_key,
                cx,
                cy,
            ) {
                continue;
            }
            if !filled {
                // 边界：至少一个 4-邻格不在图形内（用同样的图形谓词判定，避免形状相关特判）
                let neighbors = [
                    (cx - snap, cy),
                    (cx + snap, cy),
                    (cx, cy - 1.0),
                    (cx, cy + 1.0),
                ];
                let on_boundary = neighbors.iter().any(|&(nx, ny)| {
                    !point_in_shape(
                        kind,
                        rect,
                        shift_constrained,
                        px_per_tick,
                        px_per_key,
                        nx,
                        ny,
                    )
                });
                if !on_boundary {
                    continue;
                }
            }
            cells.push((cx, cy as u16));
        }
    }
    cells
}
