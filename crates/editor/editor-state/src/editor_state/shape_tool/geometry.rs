//! 形状工具几何计算：外接框规范化、屏幕空间正图形约束、顶点、内外判定、格点与描边折线

use super::{ShapeKind, ShapeSpec};

/// 圆形**描边折线**的采样段数（与预览渲染的 64 段同量级）
///
/// 只决定椭圆被折线逼近的精细度：`path_notes` 对折线跨音高行的段是**解析展开**的
/// （逐行插值求交点），不会因段数少而漏行，故无需随缩放/半径自适应。
pub const CIRCLE_OUTLINE_SEGMENTS: usize = 64;

/// 正图形约束的最小屏幕尺度（像素）：防除零 / NaN，也保证退化拖拽仍有可判定的几何
const MIN_CONSTRAINT_SCALE: f32 = 1e-6;

/// 规范化外接框：保证 lo <= hi（与拖拽方向无关）
pub(super) fn normalize_rect(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    let (x0, x1) = if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) };
    let (y0, y1) = if a.1 <= b.1 { (a.1, b.1) } else { (b.1, a.1) };
    (x0, y0, x1, y1)
}

/// 拖拽方向符号：**零位移按正向处理**
///
/// 规范化后的外接框（[`normalize_rect`]，`ShapeInstance::rect` 已保证）两轴位移恒非负，
/// 这里只为「单轴拖拽 + Shift」保留一个确定的延伸方向：`f32::signum(0.0) == 0.0`
/// 会把算好的边长乘成 0，正图形又退化成一条发丝。
fn axis_dir(d: f32) -> f32 {
    if d < 0.0 { -1.0 } else { 1.0 }
}

/// 屏幕空间约束尺度取小：**零位移轴不参与取值**
///
/// 单轴拖拽（另一轴位移为 0）时 `min(宽_px, 高_px)` 会退化成 0，把正图形压成
/// 一条发丝（矩形）/ 一个点（圆）——零位移轴提供不了尺度，故只在两轴都有位移时取 min；
/// 两轴都无位移才回落到 [`MIN_CONSTRAINT_SCALE`]。
fn constraint_scale(a: f32, b: f32) -> f32 {
    let (a, b) = (a.abs(), b.abs());
    let scale = match (a > 0.0, b > 0.0) {
        (true, true) => a.min(b),
        (true, false) => a,
        (false, true) => b,
        (false, false) => 0.0,
    };
    scale.max(MIN_CONSTRAINT_SCALE)
}

/// 应用 Shift 约束后的圆外接框（屏幕像素空间正圆：rx_px = ry_px = min）
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数。
/// 在**屏幕空间**取半径像素最小值再换算回逻辑 tick/key，保证屏幕上呈现正圆
/// （单轴拖拽时取有位移的那一轴，见 [`constraint_scale`]）。
fn screen_circle_rect(
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    px_per_tick: f32,
    px_per_key: f32,
) -> (f32, f32, f32, f32) {
    let mx = (x0 + x1) / 2.0;
    let my = (y0 + y1) / 2.0;
    let rx_px = ((x1 - x0) / 2.0).abs() * px_per_tick;
    let ry_px = ((y1 - y0) / 2.0).abs() * px_per_key;
    let r = constraint_scale(rx_px, ry_px);
    let rx = r / px_per_tick;
    let ry = r / px_per_key;
    (mx - rx, my - ry, mx + rx, my + ry)
}

/// 应用 Shift 约束后的矩形外接框（屏幕像素空间正方形）
///
/// 在屏幕空间取 min(宽_px, 高_px) 作为边长（单轴拖拽时取有位移的那一轴，见
/// [`constraint_scale`]），再换算回逻辑 tick/key 尺寸，这样拉出的矩形在屏幕上才是
/// 真正的正方形（不再被压扁）。边长带方向符号（[`axis_dir`]）。
fn screen_square_rect(
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    px_per_tick: f32,
    px_per_key: f32,
) -> (f32, f32, f32, f32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let side = constraint_scale(dx * px_per_tick, dy * px_per_key);
    let w = side / px_per_tick; // 逻辑 tick 边长（保留 dx 方向）
    let h = side / px_per_key; // 逻辑 key 边长（保留 dy 方向）
    (x0, y0, x0 + axis_dir(dx) * w, y0 + axis_dir(dy) * h)
}

/// 应用 Shift 约束后的三角形外接框（屏幕像素空间等边三角形）
///
/// 以矩形底边（沿 tick 的水平边、**拖拽当前点**所在的 key 边）宽度为基准，
/// 屏幕高度 = 底宽_px × √3 / 2，再换算回逻辑 key 高度，保证屏幕上呈现真正的
/// 等边三角形；顶点从底边朝**拖拽起点**那一侧（`apex_high`）延伸——与未约束时的朝向
/// 约定一致（见 [`normal_triangle_verts`]）。
/// 纯竖直拖拽（底边宽为 0）时改由拖拽高度反推所需底宽（h × 2 / √3），
/// 避免约束把三角形压成一条发丝。
fn screen_equilateral_rect(
    (x0, y0, x1, y1): (f32, f32, f32, f32),
    px_per_tick: f32,
    px_per_key: f32,
    apex_high: bool,
) -> (f32, f32, f32, f32) {
    let dx = x1 - x0;
    let width_px = dx.abs() * px_per_tick;
    let (base_ticks, base_px) = if width_px > 0.0 {
        (dx, width_px)
    } else {
        let px = (y1 - y0).abs() * px_per_key * 2.0 / (3.0_f32).sqrt();
        (px / px_per_tick, px)
    };
    // 等边三角形：高 = 底宽 × √3 / 2
    let h = (base_px * (3.0_f32).sqrt() / 2.0 / px_per_key).max(MIN_CONSTRAINT_SCALE);
    // 底边留在拖拽当前点那一侧，顶点朝起点那一侧延伸恰好 h（外接框仍保持规范序）
    if apex_high {
        (x0, y0, x0 + base_ticks, y0 + h)
    } else {
        (x0, y1 - h, x0 + base_ticks, y1)
    }
}

/// 三角形三顶点（未约束：顶点在 `apex_high` 指定的一侧 key 边，底边在另一侧）
///
/// **朝向约定**：顶点在 key **大**的一侧（`apex_high = true`）时，横向卷帘下屏幕上
/// 呈**正立**三角形；顶点在 key 小的一侧时呈**倒立**。key 轴正向在屏幕上是「音高更高」
/// 的方向（横向卷帘见 `ViewState::key_to_y`、纵向卷帘见 `Editor::tick_key_to_pos_f32`：
/// key 越大越靠屏幕上方 / 靠右），故纵向卷帘下「正立」表现为顶点朝右。
///
/// `apex_high` 由**拖拽方向**决定（顶点朝拖拽起点侧，见 [`ShapeSpec::apex_high`]），
/// 不是固定常量：历史上先后被钉在 key 小的一侧（永远倒三角）与 key 大的一侧（永远正立）
/// ——两次都是把「方向」当常量；方向必须由 `normalize_rect` 之外的一等字段承载。
///
/// 外接框需已规范化（[`normalize_rect`]：y0 <= y1），本函数不做大小兜底。
fn normal_triangle_verts(rect: (f32, f32, f32, f32), apex_high: bool) -> [(f32, f32); 3] {
    let (x0, y0, x1, y1) = rect;
    let mid = (x0 + x1) / 2.0;
    let (base_key, apex_key) = if apex_high { (y0, y1) } else { (y1, y0) };
    [(x0, base_key), (x1, base_key), (mid, apex_key)]
}

/// 形状对外有效外接框：统一应用 Shift 正图形约束（**屏幕像素空间**）
///
/// - 矩形 → 屏幕正方形（min(宽_px, 高_px)）；
/// - 圆 → 屏幕正圆（rx_px = ry_px）；
/// - 三角形 → 屏幕等边三角形（高 = 底宽_px × √3 / 2，顶点朝拖拽起点侧）。
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数：
/// 逻辑空间里 tick 与 key 的像素尺度悬殊，直接在逻辑空间取 min(宽,高) 会让矩形
/// 在屏幕上被压扁（X 向宽度异常压缩、且因 X 被钳到 Y 而不跟手），故约束必须放到
/// 屏幕空间做。几何判定 / 渲染 / 命中测试均应先经此函数并传入相同尺度，保证三者一致。
///
/// 外接框假定已规范化（[`normalize_rect`]：x0 <= x1、y0 <= y1；`ShapeInstance::rect`
/// 已保证），本函数不对大小关系做兜底——正图形的朝向约定基于「key 大 = 音高高」。
pub fn effective_rect(spec: ShapeSpec, px_per_tick: f32, px_per_key: f32) -> (f32, f32, f32, f32) {
    let ShapeSpec {
        kind,
        rect,
        shift_constrained,
        apex_high,
    } = spec;
    if !shift_constrained {
        return rect;
    }
    // 防止缩放尺度为 0 时换算出现除零 / NaN
    let px_per_tick = px_per_tick.max(1e-6);
    let px_per_key = px_per_key.max(1e-6);
    match kind {
        ShapeKind::Rectangle => screen_square_rect(rect, px_per_tick, px_per_key),
        ShapeKind::Circle => screen_circle_rect(rect, px_per_tick, px_per_key),
        ShapeKind::Triangle => screen_equilateral_rect(rect, px_per_tick, px_per_key, apex_high),
    }
}

/// 形状多边形顶点（逻辑坐标），用于矢量渲染
///
/// - 矩形：返回 4 个角；
/// - 三角形：返回 3 个顶点（顶点朝拖拽起点侧，见 [`ShapeSpec::apex_high`]；
///   Shift 约束为等边）；
/// - 圆形：无顶点（用椭圆渲染），返回 `None`。
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数，
/// 用于屏幕空间的正图形约束（见 `effective_rect`）。
pub fn shape_vertices(
    spec: ShapeSpec,
    px_per_tick: f32,
    px_per_key: f32,
) -> Option<Vec<(f32, f32)>> {
    let rect = effective_rect(spec, px_per_tick, px_per_key);
    match spec.kind {
        ShapeKind::Rectangle => {
            let (x0, y0, x1, y1) = rect;
            Some(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
        }
        ShapeKind::Triangle => Some(normal_triangle_verts(rect, spec.apex_high).to_vec()),
        ShapeKind::Circle => None,
    }
}

/// 判断逻辑坐标点 (cx, cy) 是否落在指定图形内部
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数，
/// 用于屏幕空间的正图形约束（见 `effective_rect`）。
pub fn point_in_shape(
    spec: ShapeSpec,
    px_per_tick: f32,
    px_per_key: f32,
    cx: f32,
    cy: f32,
) -> bool {
    let rect = effective_rect(spec, px_per_tick, px_per_key);
    match spec.kind {
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
            let verts = normal_triangle_verts(rect, spec.apex_high);
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
/// - `filled = true`：图形内部全部格点（**形状工具的填充腿走这一支**）；
/// - `filled = false`：仅边界格点（空心图形的网格轮廓）。
///
/// ⚠️ **描边（轮廓）的音符生成不走本函数**：形状工具的描边改走
/// [`shape_outline_path`]（边界连续几何）→ 蜘蛛网式逐音高行解析，与曲线工具轮廓同源
/// （见 `ui-editor` 的 `interaction/shape_tool.rs`）；`filled = false` 仅保留为
/// 「按 snap 网格枚举轮廓格点」这一几何查询本身，供其它调用方使用。
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数，
/// 用于屏幕空间的正图形约束（见 `effective_rect`）。
pub fn shape_cells(
    spec: ShapeSpec,
    filled: bool,
    snap: f32,
    px_per_tick: f32,
    px_per_key: f32,
) -> Vec<(f32, u16)> {
    let snap = snap.max(1.0);
    let (x0, y0, x1, y1) = spec.rect;
    let xi0 = (x0 / snap).floor() as i64;
    let xi1 = (x1 / snap).ceil() as i64;
    let yi0 = y0.floor() as i64;
    let yi1 = y1.ceil() as i64;
    let mut cells = Vec::new();
    for xi in xi0..=xi1 {
        let cx = xi as f32 * snap;
        for yi in yi0..=yi1 {
            let cy = yi as f32;
            if !point_in_shape(spec, px_per_tick, px_per_key, cx, cy) {
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
                let on_boundary = neighbors
                    .iter()
                    .any(|&(nx, ny)| !point_in_shape(spec, px_per_tick, px_per_key, nx, ny));
                if !on_boundary {
                    continue;
                }
            }
            cells.push((cx, cy as u16));
        }
    }
    cells
}

/// 形状**描边**（轮廓）折线，逻辑坐标 `(tick, key)`，**闭合环：末点 == 首点**
///
/// 与 [`shape_cells`] 的「按 snap 网格枚举轮廓格点」相对：这里给出的是形状边界的
/// **连续几何**，供「音符生成」走与曲线工具轮廓同源的蜘蛛网式逐音高行解析
/// （`ui-editor` 的 `interaction::line_tool::paths::path_notes`）——每个音高行一条
/// 音符、两两无缝连奏、长度由边界与行边界的解析交点决定，不再依赖吸附精度。
///
/// - 矩形 / 三角形：顶点 + 闭合点（顶点序与 [`shape_vertices`] 一致，
///   闭合环如何起头交给 `path_notes` 的 `loop_from_left` 决定）；
/// - 圆：按椭圆参数采样 [`CIRCLE_OUTLINE_SEGMENTS`] 段 + 闭合点。末点**显式取首点**
///   而不是再算一次 `cos(TAU)`——`path_notes` 靠 `pts[0] == pts[last]` 识别闭合环，
///   浮点采样下 `cos(TAU)` 未必逐位等于 `cos(0)`，差一位就会退化按开放路径处理
///   （多做首尾拉伸，接缝处留下接痕）。
///
/// `px_per_tick` / `px_per_key` 为卷帘 X(tick) / Y(key) 方向每单位像素数，
/// 用于屏幕空间的正图形约束（见 [`effective_rect`]）。渲染与生成必须传同一组尺度，
/// 否则「看到的图形」与「生成的音符」分叉。
pub fn shape_outline_path(spec: ShapeSpec, px_per_tick: f32, px_per_key: f32) -> Vec<(f32, f32)> {
    let rect = effective_rect(spec, px_per_tick, px_per_key);
    let mut out: Vec<(f32, f32)> = match spec.kind {
        ShapeKind::Rectangle => {
            let (x0, y0, x1, y1) = rect;
            vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        }
        ShapeKind::Triangle => normal_triangle_verts(rect, spec.apex_high).to_vec(),
        ShapeKind::Circle => {
            let (x0, y0, x1, y1) = rect;
            let mx = (x0 + x1) / 2.0;
            let my = (y0 + y1) / 2.0;
            let rx = (x1 - x0) / 2.0;
            let ry = (y1 - y0) / 2.0;
            (0..CIRCLE_OUTLINE_SEGMENTS)
                .map(|i| {
                    let a = (i as f32 / CIRCLE_OUTLINE_SEGMENTS as f32) * std::f32::consts::TAU;
                    (mx + rx * a.cos(), my + ry * a.sin())
                })
                .collect()
        }
    };
    // 显式闭合（理由见函数文档：闭合环判定是逐位相等）
    if let Some(&first) = out.first() {
        out.push(first);
    }
    out
}
