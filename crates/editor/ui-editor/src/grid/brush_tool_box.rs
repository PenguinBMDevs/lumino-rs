//! 画刷矢量笔画渲染：分层实体 + **聚合圆头圆尾** + 共享 √× 悬浮按钮
//!
//! 视觉规格（卡面）：
//! - 粗细度 1 = 单个 key 高度（`zoom_y`），其他粗细度 = 粗细度 × key 高度，
//!   缩放/滚动时按当前视图实时重算几何（线宽随之变化）；
//! - 整条笔画（含全部粗细度层）**只聚合出一对圆头/圆尾**：端帽半径 = 笔画总宽 / 2，
//!   端帽按层色分段（同一对圆帽被 key 边界硬切成若干色块），不按层各画一对端帽；
//! - 层色 = 该层分配音轨的音符显示色加深 40%（含洋葱皮轨），key 边界**硬切换**，
//!   颜色在每次绘制时实时取色 → 跟随音轨/调色板设置动态变化。
//!
//! 性能（卡面的硬约束）：
//! - 视口裁剪（整笔在图外直接跳过；部分可见只取可见区间两侧各多一个点）；
//! - 屏幕空间抽稀（位移小于 `zoom_y × DECIMATE_FACTOR` 的采样点合并）+ 单笔点上限；
//! - 每帧几何量有上界（点数上限 × 层数 + 2 个端帽段），不随笔画长度线性劣化。
//!
//! 渲染路径选型：canvas 叠加层（与 `line_tool_box` 同范式）——圆头/圆角由
//! `LineCap::Round` / `LineJoin::Round` 直接支持，无需 GFX 管线与跨线程同步。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::content_bounds;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, LineCap, LineJoin, Path, Stroke};
use lumino_message::Tool;
use lumino_ui_core::Renderer;

/// 按钮组与笔画包围盒的间距
const BUTTON_SPACING: f32 = 8.0;
/// 单条笔画预览点上限（超过则等距抽稀；生成用的点列不受影响）
const MAX_PREVIEW_POINTS: usize = 512;
/// 屏幕空间抽稀阈值系数（× 单 key 高度）
const DECIMATE_FACTOR: f32 = 0.35;
/// 端帽圆弧分段数（每层每半弧）
const CAP_SEGMENTS: usize = 8;

/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct BrushButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 计算待确认笔画右侧共享悬浮按钮位置（垂直居中于笔画包围盒中心）
///
/// 注意：**不**按 `current_track == 0` 隐藏按钮（与曲线工具不同）——
/// 笔画的目标音轨由**层分配**决定（含落笔时记录的基准轨），与当前轨无关；
/// 若照抄曲线工具的 Conductor 隐藏规则，用户在待确认期间切到 Conductor 轨
/// 会导致笔画被卡死（既不能 √ 也不能 ×）。Conductor 只在**落笔**时拦截（`pressed.rs`）。
pub fn brush_button_rects(editor: &Editor) -> Option<BrushButtonRects> {
    if editor.current_tool() != Tool::Brush {
        return None;
    }
    if !editor.editor_state.brush_tool.has_pending() {
        return None;
    }
    let (min_x, max_x, min_y, max_y) = editor.brush_strokes_screen_bounds()?;
    let content = content_bounds(editor);
    if content.height < BUTTON_SIZE {
        return None;
    }
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;

    let group_w = BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    // 垂直中心钳制到内容区内，避免笔画 Y 向越界时按钮悬浮到键盘/标尺上方
    let center_y = mid_y.clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    // 水平位置：优先包围盒右侧，超出内容区右边缘时钳制到右边缘
    let x0 = (mid_x + BUTTON_SPACING).min(content.x + content.width - group_w - BUTTON_SPACING);
    if x0 < content.x + BUTTON_SPACING {
        return None;
    }
    let y0 = center_y - BUTTON_SIZE * 0.5;
    let confirm = Rectangle::new(Point::new(x0, y0), Size::new(BUTTON_SIZE, BUTTON_SIZE));
    let cancel = Rectangle::new(
        Point::new(x0 + BUTTON_SIZE + BUTTON_SPACING, y0),
        Size::new(BUTTON_SIZE, BUTTON_SIZE),
    );
    Some(BrushButtonRects { confirm, cancel })
}

/// 绘制全部待确认笔画（分层实体 + 聚合端帽）+ 共享 √× 悬浮按钮
///
/// 仅在画刷工具激活时绘制。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    _theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    if editor.current_tool() != Tool::Brush {
        return None;
    }
    if !editor.editor_state.brush_tool.has_pending() {
        return None;
    }
    let thickness = editor.brush.thickness.max(1) as usize;
    let zoom_y = editor.editor_state.view.zoom_y;
    let zh = zoom_y.max(0.5);
    let total_width = editor.brush_total_width_px();

    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;
    for stroke in &editor.editor_state.brush_tool.strokes {
        let points = preview_points(editor, &stroke.points, bounds, total_width);
        if points.is_empty() {
            continue;
        }
        if points.len() == 1 {
            draw_single_point_block(
                &mut frame,
                editor,
                points[0],
                stroke.base_track,
                thickness,
                zh,
            );
            has_content = true;
            continue;
        }
        draw_stroke(
            &mut frame,
            editor,
            &points,
            stroke.base_track,
            thickness,
            zh,
            total_width,
        );
        has_content = true;
    }

    if let Some(btns) = brush_button_rects(editor) {
        draw_button(
            &mut frame,
            btns.confirm,
            &CONFIRM_ICON,
            iced_core::Color::from_rgb8(46, 125, 50),
        );
        draw_button(
            &mut frame,
            btns.cancel,
            &CANCEL_ICON,
            iced_core::Color::from_rgb8(198, 40, 40),
        );
        has_content = true;
    }

    if has_content {
        Some(frame.into_geometry())
    } else {
        None
    }
}

/// 绘制一条折线笔画：逐层实体（层带硬切换）+ 两个聚合端帽
#[allow(clippy::too_many_arguments)]
fn draw_stroke(
    frame: &mut canvas::Frame<Renderer>,
    editor: &Editor,
    points: &[Point],
    base_track: usize,
    thickness: usize,
    zh: f32,
    total_width: f32,
) {
    // 逐层实体：把折线整体上移 (level + 0.5) 个 key 高，用 1 个 key 高画粗线 →
    // 各层正好占据 [key+level, key+level+1) 这个 key 带（端帽统一最后画）
    for level in 0..thickness {
        let color = editor.brush_layer_color(level, base_track);
        let offset = zh * (level as f32 + 0.5);
        let path = Path::new(|p| {
            for (index, point) in points.iter().enumerate() {
                let q = Point::new(point.x, point.y - offset);
                if index == 0 {
                    p.move_to(q);
                } else {
                    p.line_to(q);
                }
            }
        });
        let stroke = Stroke::default()
            .with_width(zh)
            .with_color(color)
            .with_line_cap(LineCap::Butt)
            .with_line_join(LineJoin::Round);
        frame.stroke(&path, stroke);
    }

    // 聚合端帽：整条笔画只有一对圆帽（半径 = 总宽 / 2），按层带切成色块
    let radius = total_width * 0.5;
    for point in [points[0], points[points.len() - 1]] {
        let center = Point::new(point.x, point.y - radius);
        for level in 0..thickness {
            let color = editor.brush_layer_color(level, base_track);
            let y_hi = point.y - zh * level as f32;
            let y_lo = y_hi - zh;
            let path = cap_segment_path(center, radius, y_lo, y_hi);
            frame.fill(&path, color);
        }
    }
}

/// 绘制单点笔画（点一下不拖）：按"一个网格单元宽 × 粗细度 key 高"的色块
///
/// 与生成结果一致（该格生成 thickness 个音符，长度 = 吸附精度），
/// 因此不画圆帽（不存在"线"的端部）。
fn draw_single_point_block(
    frame: &mut canvas::Frame<Renderer>,
    editor: &Editor,
    point: Point,
    base_track: usize,
    thickness: usize,
    zh: f32,
) {
    let cell_w =
        (editor.editor_state.view.snap_precision * editor.editor_state.view.zoom_x).max(1.0);
    for level in 0..thickness {
        let color = editor.brush_layer_color(level, base_track);
        let y_hi = point.y - zh * level as f32;
        let rect = Rectangle::new(
            Point::new(point.x - cell_w * 0.5, y_hi - zh),
            Size::new(cell_w, zh),
        );
        let path = Path::rectangle(rect.position(), rect.size());
        frame.fill(&path, color);
    }
}

/// 构造端帽的一段：圆（圆心 `center`、半径 `radius`）在 y ∈ [y_lo, y_hi] 之间的色块
///
/// 由左右两段圆弧 + 上下两条弦闭合而成；同一对端帽的各级段拼起来 = 完整圆帽。
fn cap_segment_path(center: Point, radius: f32, y_lo: f32, y_hi: f32) -> Path {
    let unit = |v: f32| (v / radius).clamp(-1.0, 1.0);
    let theta_lo = unit(y_lo - center.y).asin();
    let theta_hi = unit(y_hi - center.y).asin();
    let arc = |mirror: bool| -> Vec<Point> {
        (0..=CAP_SEGMENTS)
            .map(|i| {
                let t = i as f32 / CAP_SEGMENTS as f32;
                let theta = if mirror {
                    theta_hi - (theta_hi - theta_lo) * t
                } else {
                    theta_lo + (theta_hi - theta_lo) * t
                };
                let x = if mirror {
                    center.x - radius * theta.cos()
                } else {
                    center.x + radius * theta.cos()
                };
                Point::new(x, center.y + radius * theta.sin())
            })
            .collect()
    };
    let right = arc(false);
    let left = arc(true);
    Path::new(move |p| {
        let mut first = true;
        for point in right.iter().chain(left.iter()) {
            if first {
                p.move_to(*point);
                first = false;
            } else {
                p.line_to(*point);
            }
        }
    })
}

/// 逻辑点列 → 预览屏幕点列：**逻辑空间预裁剪** + 屏幕空间抽稀 + 点数上限
///
/// 生成用的**无损**点列（`BrushStroke.points`）不经过本函数，
/// 因此覆盖正确性不依赖抽稀强度。
///
/// 性能：先按逻辑视口区间（由画布四角反推，含 1 格余量）取可见子区间，
/// 只对该子区间做屏幕映射——长笔画滚出视口后每帧成本与总点数解耦。
///
/// `pub`：供渲染层测试与性能基准（`benches/ui_brush_stroke_bench.rs`）复用。
pub fn preview_points(
    editor: &Editor,
    logical: &[(f32, f32)],
    bounds: Rectangle,
    total_width: f32,
) -> Vec<Point> {
    if logical.is_empty() {
        return Vec::new();
    }
    let pad = total_width + 1.0;
    let p0 = Point::new(bounds.x - pad, bounds.y - pad);
    let p1 = Point::new(
        bounds.x + bounds.width + pad,
        bounds.y + bounds.height + pad,
    );
    // 视图映射对 tick/key 单调：用画布对角反推逻辑区间（含 1 格余量）
    let (ta, tb) = (editor.pos_to_tick(p0), editor.pos_to_tick(p1));
    let (ka, kb) = (editor.pos_to_key(p0), editor.pos_to_key(p1));
    let (t_lo, t_hi) = (ta.min(tb) - 1.0, ta.max(tb) + 1.0);
    let (k_lo, k_hi) = (ka.min(kb) as f32 - 1.0, ka.max(kb) as f32 + 1.0);
    let in_view = |tick: f32, key: f32| tick >= t_lo && tick <= t_hi && key >= k_lo && key <= k_hi;
    let (Some(first), Some(last)) = (
        logical.iter().position(|&(t, k)| in_view(t, k)),
        logical.iter().rposition(|&(t, k)| in_view(t, k)),
    ) else {
        return Vec::new(); // 整笔在视口外
    };
    let slice = &logical[first.saturating_sub(1)..=(last + 1).min(logical.len() - 1)];
    let screen: Vec<Point> = slice
        .iter()
        .map(|&p| editor.line_pos_screen_pos(p))
        .collect();

    // 抽稀：位移小于 zoom_y × 系数 的采样点合并（远小于一个 key，视觉无差别）
    let step = (editor.editor_state.view.zoom_y * DECIMATE_FACTOR).max(0.5);
    let mut out: Vec<Point> = Vec::with_capacity(screen.len().min(MAX_PREVIEW_POINTS));
    for &point in &screen {
        match out.last() {
            None => out.push(point),
            Some(&prev) => {
                if (point.x - prev.x).hypot(point.y - prev.y) >= step {
                    out.push(point);
                }
            }
        }
    }
    // 终点必须保留（端帽位置 = 实际笔画终点）
    if let Some(&end) = screen.last()
        && out.last() != Some(&end)
    {
        out.push(end);
    }
    // 硬上限：等距抽样（首尾保留）
    if out.len() > MAX_PREVIEW_POINTS {
        out = uniform_sample(&out, MAX_PREVIEW_POINTS);
    }
    out
}

/// 等距抽样到 `max` 个点（保留首尾）
fn uniform_sample(points: &[Point], max: usize) -> Vec<Point> {
    if max < 2 || points.len() <= max {
        return points.to_vec();
    }
    let last = points.len() - 1;
    (0..max).map(|i| points[i * last / (max - 1)]).collect()
}

#[cfg(test)]
mod tests;
