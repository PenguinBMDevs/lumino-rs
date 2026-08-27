//! 曲线工具贝塞尔路径渲染：多路径锚点 + 曲线段 + 控制柄 + 实心填充 + 共享 √× 按钮
//!
//! 支持多条路径同时存在（批量绘制）：
//! - 锚点：实心圆（醒目标注，端点可拖动、中间锚点自由移动）；
//! - 曲线段：3-4 像素粗三次贝塞尔（控制柄与锚点重合时退化为直线）；
//! - 控制柄：方块 + 锚点间辅助线（首锚点 out、尾锚点 in、中间 in+out），
//!   拖动控制柄弯曲曲线；
//! - 实心填充：颜料桶点击封闭区域后，内部格点以半透明色块显示
//!   （√ 确认时合并生成实心音符）；
//! - 按钮：全部路径包围盒右侧的一组 √（批量确认生成音符）/ ×（批量取消）
//!   悬浮按钮，与 i2m 区域框按钮共用同一套视觉（`confirm_buttons` 模块）。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::content_bounds;
use crate::interaction::line_tool::fill::region::{FillRegion, fill_region};
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_message::Tool;
use lumino_ui_core::Renderer;

/// 曲线段粗细（像素，用户要求 3-4 像素，取 4）
const LINE_WIDTH: f32 = 4.0;
/// 锚点半径（像素）
const ANCHOR_RADIUS: f32 = 6.0;
/// 锚点描边宽度（像素）
const ANCHOR_STROKE_WIDTH: f32 = 2.0;
/// 控制柄方块边长（像素）
const HANDLE_SIZE: f32 = 8.0;
/// 控制柄辅助线宽度（像素）
const HANDLE_LINE_WIDTH: f32 = 1.0;
/// 按钮组与路径包围盒的间距
const BUTTON_SPACING: f32 = 8.0;
/// 填充色块透明度（半透明色块叠在网格上，√ 确认后成为实心音符）
const FILL_ALPHA: f32 = 0.35;

/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct LineButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 全部路径锚点的屏幕包围盒 (min_x, max_x, min_y, max_y)
fn paths_bounds(editor: &Editor) -> Option<(f32, f32, f32, f32)> {
    let paths = &editor.editor_state.line_tool.paths;
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for path in paths {
        for anchor in path {
            let p = editor.line_pos_screen_pos(anchor.pos);
            bounds = Some(match bounds {
                None => (p.x, p.x, p.y, p.y),
                Some((min_x, max_x, min_y, max_y)) => (
                    min_x.min(p.x),
                    max_x.max(p.x),
                    min_y.min(p.y),
                    max_y.max(p.y),
                ),
            });
        }
    }
    bounds
}

/// 计算全部路径右侧共享悬浮按钮位置（垂直居中于包围盒中心）
///
/// 按钮组钳制到卷帘内容区内：路径移出/越界时按钮仍保持完整可见可点。
pub fn line_button_rects(editor: &Editor) -> Option<LineButtonRects> {
    // Conductor 音轨（track 0）：整工具不可用，不计算 √× 按钮命中区
    if editor.editor_state.data.current_track == 0 {
        return None;
    }
    if editor.current_tool() != Tool::Curve {
        return None;
    }
    // 至少存在一条完整路径才显示按钮
    if !editor.editor_state.line_tool.is_complete() {
        return None;
    }
    let (min_x, max_x, min_y, max_y) = paths_bounds(editor)?;
    let content = content_bounds(editor);
    // 内容区高度不足以容纳单个按钮时（异常布局）不显示按钮
    if content.height < BUTTON_SIZE {
        return None;
    }
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;

    let group_w = BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    // 垂直中心钳制到内容区内，避免路径 Y 向越界时按钮悬浮到键盘/标尺上方
    let center_y = mid_y.clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    // 水平位置：优先包围盒右侧，超出内容区右边缘时钳制到右边缘
    let x0 = (mid_x + BUTTON_SPACING).min(content.x + content.width - group_w - BUTTON_SPACING);
    // 内容区过窄无法容纳按钮组时（异常布局）不显示按钮
    if x0 < content.x + BUTTON_SPACING {
        return None;
    }
    let y0 = center_y - BUTTON_SIZE * 0.5;
    let confirm = Rectangle::new(Point::new(x0, y0), Size::new(BUTTON_SIZE, BUTTON_SIZE));
    let cancel = Rectangle::new(
        Point::new(x0 + BUTTON_SIZE + BUTTON_SPACING, y0),
        Size::new(BUTTON_SIZE, BUTTON_SIZE),
    );
    Some(LineButtonRects { confirm, cancel })
}

/// 绘制全部路径（锚点 + 贝塞尔段 + 控制柄）+ 共享 √× 悬浮按钮
///
/// 仅在曲线工具激活时绘制。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    // Conductor 音轨（track 0）：整工具不可用，不绘制路径与 √× 按钮
    if editor.editor_state.data.current_track == 0 {
        return None;
    }
    if editor.current_tool() != Tool::Curve {
        return None;
    }
    let line = &editor.editor_state.line_tool;
    if line.paths.is_empty() {
        return None;
    }

    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;
    let anchor_color = theme.extended_palette().primary.strong.color;
    // 控制柄辅助线：主题色 50% 透明度
    let handle_line_color =
        iced_core::Color::from_rgba(anchor_color.r, anchor_color.g, anchor_color.b, 0.5);
    let white = iced_core::Color::WHITE;

    for (pi, path) in line.paths.iter().enumerate() {
        // 曲线段（贝塞尔；控制柄与锚点重合时退化为直线）
        if path.len() >= 2 {
            for pair in path.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let pa = editor.line_pos_screen_pos(a.pos);
                let p1 = editor.line_pos_screen_pos(a.out_handle_abs());
                let p2 = editor.line_pos_screen_pos(b.in_handle_abs());
                let pb = editor.line_pos_screen_pos(b.pos);
                let curve = Path::new(|p| {
                    p.move_to(pa);
                    p.bezier_curve_to(p1, p2, pb);
                });
                let stroke = Stroke::default()
                    .with_width(LINE_WIDTH)
                    .with_color(anchor_color);
                frame.stroke(&curve, stroke);
                has_content = true;
            }
        }

        // 控制柄（辅助线 + 方块，常驻显示）
        for (ai, anchor) in path.iter().enumerate() {
            let ap = editor.line_pos_screen_pos(anchor.pos);
            for side in line.visible_handle_sides(pi, ai) {
                let h_abs = match side {
                    lumino_editor_state::HandleSide::In => anchor.in_handle_abs(),
                    lumino_editor_state::HandleSide::Out => anchor.out_handle_abs(),
                };
                let hp = editor.line_pos_screen_pos(h_abs);
                // 锚点 → 控制柄辅助线
                let handle_connector = Path::new(|p| {
                    p.move_to(ap);
                    p.line_to(hp);
                });
                let connector_stroke = Stroke::default()
                    .with_width(HANDLE_LINE_WIDTH)
                    .with_color(handle_line_color);
                frame.stroke(&handle_connector, connector_stroke);
                // 控制柄方块
                let rect = Rectangle::new(
                    Point::new(hp.x - HANDLE_SIZE * 0.5, hp.y - HANDLE_SIZE * 0.5),
                    Size::new(HANDLE_SIZE, HANDLE_SIZE),
                );
                let path = Path::rectangle(rect.position(), rect.size());
                frame.fill(&path, anchor_color);
                let ring = Stroke::default().with_width(1.0).with_color(white);
                frame.stroke(&path, ring);
                has_content = true;
            }
        }

        // 锚点：实心圆 + 白色描边（明确标注路径锚点）
        for anchor in path {
            let ap = editor.line_pos_screen_pos(anchor.pos);
            let path = Path::circle(ap, ANCHOR_RADIUS);
            frame.fill(&path, anchor_color);
            let ring = Stroke::default()
                .with_width(ANCHOR_STROKE_WIDTH)
                .with_color(white);
            frame.stroke(&path, ring);
            has_content = true;
        }
    }

    // 颜料桶矢量填充：闭环几何（边缘 = 实际曲线轮廓，与精度/key 网格无关）
    if let Some(region) = fill_region(editor) {
        let fill_color =
            iced_core::Color::from_rgba(anchor_color.r, anchor_color.g, anchor_color.b, FILL_ALPHA);
        let path = build_fill_path(editor, &region);
        frame.fill(&path, fill_color);
        has_content = true;
    }

    // 共享悬浮按钮（存在完整路径后显示）
    if let Some(btns) = line_button_rects(editor) {
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

/// 构建填充路径（画布坐标）：填充边缘 = 闭环几何轮廓本身
///
/// iced canvas 填充默认 NonZero（绕数）规则，与填充判定的 winding 一致：
/// - 背景模式：顺时针范围矩形 + 全部闭环**反向**（洞）+ 已填闭环原方向
///   （矩形 +1、反向环 -1 → 未填环内为 0 = 洞；已填环再 +1 → 填充）；
/// - 内部模式：只画已填闭环（原方向）。
fn build_fill_path(editor: &Editor, region: &FillRegion) -> Path {
    Path::new(|p| {
        if region.has_background {
            let (min_t, min_k, max_t, max_k) = region.bounds;
            let a = editor.line_pos_screen_pos((min_t, min_k));
            let b = editor.line_pos_screen_pos((max_t, max_k));
            // 背景范围矩形（顺时针 → 内部绕数 +1）
            p.move_to(Point::new(a.x, a.y));
            p.line_to(Point::new(b.x, a.y));
            p.line_to(Point::new(b.x, b.y));
            p.line_to(Point::new(a.x, b.y));
            // 全部闭环反向 → 洞（未填图形内部不显示填充）
            for lp in &region.all_loops {
                draw_loop(p, lp, true, editor);
            }
            // 已填闭环原方向 → 与洞抵消后内部绕数 +1 → 显示填充
            for lp in &region.filled_loops {
                draw_loop(p, lp, false, editor);
            }
        } else {
            for lp in &region.filled_loops {
                draw_loop(p, lp, false, editor);
            }
        }
    })
}

/// 画一个环（逻辑坐标 → 画布坐标），reversed = 反向作为洞（NonZero 规则下
/// 矩形 +1 与反向环 -1 抵消 → 环内绕数 0 = 洞）
fn draw_loop(p: &mut canvas::path::Builder, pts: &[(f32, f32)], reversed: bool, editor: &Editor) {
    let n = pts.len();
    let mut idx = if reversed { n.saturating_sub(1) } else { 0 };
    let mut first = true;
    loop {
        let (tx, ty) = pts[idx];
        let s = editor.line_pos_screen_pos((tx, ty));
        if first {
            p.move_to(Point::new(s.x, s.y));
            first = false;
        } else {
            p.line_to(Point::new(s.x, s.y));
        }
        if reversed {
            if idx == 0 {
                break;
            }
            idx -= 1;
        } else {
            idx += 1;
            if idx >= n {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests;
