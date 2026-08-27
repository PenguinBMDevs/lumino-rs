//! 形状工具绘制状态（矩形 / 圆 / 三角形，拉出式拖拽绘制）
//!
//! 与曲线工具（`line_tool`）同构：拖拽拉出外接框 → 实时预览 → √ 批量确认生成音符。
//! 区别在于形状工具是「按住拖拽拉框」范式（曲线工具是「多点锚点」范式）。
//!
//! 画出的图形在确认（√）前作为临时叠加；确认后转成音符（每格一个，长度 = 吸附精度），
//! 与 `confirm_line_tool` 完全一致：形状不保存为矢量对象，而是「固化为音符」。
//!
//! 填充桶（`fill_enabled`）决定确认时是否额外生成图形内部音符：
//! 既可在拉框时开着填充桶直接拉出实心图形，也可在拉出轮廓后再次用填充桶点选填充。

/// 形状类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeKind {
    /// 矩形（Shift → 正方形）
    #[default]
    Rectangle,
    /// 圆（Shift → 正圆，rx = ry）
    Circle,
    /// 三角形（Shift → 等边三角形）
    Triangle,
}

/// 形状工具交互阶段
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ShapeToolInteraction {
    /// 无交互
    #[default]
    None,
    /// 拖拽拉框中（记录起点逻辑坐标 (tick, key)）
    Dragging {
        /// 拖拽起点逻辑坐标 (tick, key)
        start: (f32, f32),
    },
}

/// 单条待确认图形实例
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeInstance {
    /// 图形类型
    pub kind: ShapeKind,
    /// 外接框逻辑坐标 (tick_lo, key_lo, tick_hi, key_hi)（已规范化：lo <= hi）
    pub rect: (f32, f32, f32, f32),
    /// 绘制时是否按住 Shift（约束为正图形）
    pub shift_constrained: bool,
    /// 是否填充内部（颜料桶：绘制时开启或事后点击填充）
    pub filled: bool,
}

/// 形状工具状态
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapeToolState {
    /// 当前选中的图形类型（由工具栏 `current_shape` 同步）
    pub shape_kind: ShapeKind,
    /// 拖拽交互状态
    pub interaction: ShapeToolInteraction,
    /// 拖拽当前点逻辑坐标（实时预览用）
    pub drag_current: (f32, f32),
    /// 绘制时颜料桶是否开启（用于新拉出图形的 `filled` 默认值）
    pub fill_enabled: bool,
    /// 待确认图形列表（√ 确认批量生成音符 / × 清空）
    pub shapes: Vec<ShapeInstance>,
}

impl ShapeToolState {
    /// 重置整个状态（含已拉出图形与当前图形类型）
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 仅清除临时绘制状态（拖拽 / 待确认图形 / 填充桶开关），保留当前图形类型
    ///
    /// 用于切换工具时清理残留，但保留用户在工具栏选中的图形类型（矩形/圆/三角），
    /// 避免切换工具再切回后被重置为默认矩形。
    pub fn clear_pending(&mut self) {
        self.interaction = ShapeToolInteraction::None;
        self.drag_current = (0.0, 0.0);
        self.fill_enabled = false;
        self.shapes.clear();
    }

    /// 设置当前图形类型
    pub fn set_shape_kind(&mut self, kind: ShapeKind) {
        self.shape_kind = kind;
    }

    /// 设置填充桶开关
    pub fn set_fill_enabled(&mut self, enabled: bool) {
        self.fill_enabled = enabled;
    }

    /// 是否有待确认图形
    pub fn has_pending(&self) -> bool {
        !self.shapes.is_empty()
    }

    /// 是否正在拖拽
    pub fn is_dragging(&self) -> bool {
        matches!(self.interaction, ShapeToolInteraction::Dragging { .. })
    }

    /// 开始拖拽（记录起点）
    pub fn begin_drag(&mut self, start: (f32, f32)) {
        self.interaction = ShapeToolInteraction::Dragging { start };
        self.drag_current = start;
    }

    /// 更新拖拽当前点
    pub fn update_drag(&mut self, current: (f32, f32)) {
        self.drag_current = current;
    }

    /// 结束拖拽：由起点 + 当前点计算外接框，生成待确认图形
    ///
    /// - `snap`：吸附精度（tick），用于判定拖拽过短无效；
    /// - `shift_constrained`：绘制时是否按住 Shift（约束正图形）。
    /// 返回 `None` 表示拖拽过短 / 无效，已丢弃。
    pub fn end_drag(&mut self, snap: f32, shift_constrained: bool) -> Option<ShapeInstance> {
        let start = match self.interaction {
            ShapeToolInteraction::Dragging { start } => start,
            ShapeToolInteraction::None => return None,
        };
        self.interaction = ShapeToolInteraction::None;
        let current = self.drag_current;
        // 拖拽过短（小于半格）视为无效，丢弃（避免误触）
        if (current.0 - start.0).abs() < snap * 0.5 && (current.1 - start.1).abs() < 0.5 {
            return None;
        }
        let rect = normalize_rect(start, current);
        let instance = ShapeInstance {
            kind: self.shape_kind,
            rect,
            shift_constrained,
            filled: self.fill_enabled,
        };
        self.shapes.push(instance.clone());
        Some(instance)
    }

    /// 当前正在拖拽的预览图形（若有）：返回 (类型, 外接框, Shift约束, 填充)
    pub fn preview_rect(
        &self,
        shift_constrained: bool,
    ) -> Option<(ShapeKind, (f32, f32, f32, f32), bool, bool)> {
        if let ShapeToolInteraction::Dragging { start } = self.interaction {
            let rect = normalize_rect(start, self.drag_current);
            Some((self.shape_kind, rect, shift_constrained, self.fill_enabled))
        } else {
            None
        }
    }
}

/// 规范化外接框：保证 lo <= hi（与拖拽方向无关）
fn normalize_rect(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
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
                    !point_in_shape(kind, rect, shift_constrained, px_per_tick, px_per_key, nx, ny)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectangle_filled_cells() {
        // 4×4 矩形（key 60..64, tick 0..4 snap=1）：内部 16 格
        let cells = shape_cells(
            ShapeKind::Rectangle,
            (0.0, 60.0, 4.0, 64.0),
            false,
            true,
            1.0, 1.0, 1.0,
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
            1.0, 1.0, 1.0,
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
            1.0, 1.0, 1.0,
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
            1.0, 1.0, 1.0,
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
            1.0, 1.0, 1.0,
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
            1.0, 1.0, 1.0,
        );
        // 正圆半径 2，中心 (5,62)，中心格在内
        assert!(cells.contains(&(5.0, 62)));
        // 外接框右上 (10,64) 距中心 (5,2) → 椭圆方程 = (5/2)^2+(2/2)^2 = 7.25 > 1
        assert!(!cells.contains(&(10.0, 64)));
    }
}
