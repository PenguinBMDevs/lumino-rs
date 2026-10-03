//! 已绘制图形「选中高亮描边」叠加层
//!
//! 「鼠标工具」（`Tool::ShapeSelect`）选中某个已确认的绘制图形后，在其矢量轮廓上
//! 叠加一层高亮描边（外发光 + 实线双描边），与 `shape_tool_box` 的待确认预览区分：
//! 预览用主题强调色细线（2.5px），选中高亮用更粗的发光描边提示「这一整块被选中」。
//!
//! 渲染口径与命中测试同源：形状工具图形走 `shape_vertices` / 椭圆，折线
//! （曲线展平路径 / 画刷笔画）直接连点成线。仅在选中图形属于**当前音轨**且未被
//! 撤销隐藏时绘制。

use crate::Editor;
use iced_core::{Color, Point, Rectangle};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_editor_state::DrawnShapeSource;
use lumino_editor_state::shape_tool::{effective_rect, shape_vertices};
use lumino_ui_core::{Renderer, Theme};

/// 外发光描边宽度（像素）
const GLOW_WIDTH: f32 = 7.0;
/// 实线描边宽度（像素）
const CORE_WIDTH: f32 = 2.5;
/// 外发光透明度
const GLOW_ALPHA: f32 = 0.30;

/// 生成椭圆路径（本 iced 版本无 `Path::ellipse`，改用多边形逼近）
fn ellipse_path(center: Point, rx: f32, ry: f32) -> Path {
    const SEGMENTS: usize = 64;
    Path::new(|p| {
        for i in 0..=SEGMENTS {
            let a = (i as f32 / SEGMENTS as f32) * 2.0 * std::f32::consts::PI;
            let x = center.x + rx * a.cos();
            let y = center.y + ry * a.sin();
            if i == 0 {
                p.move_to(Point::new(x, y));
            } else {
                p.line_to(Point::new(x, y));
            }
        }
        p.close();
    })
}

/// 把选中图形的轮廓转换成屏幕坐标路径（闭环标记用于形状工具图形）
fn selected_outline(editor: &Editor) -> Option<(Path, bool)> {
    let shape = editor.editor_state.shape_select.selected_shape()?;
    if shape.track != editor.editor_state.data.current_track
        || editor.editor_state.shape_select.is_hidden(shape)
    {
        return None;
    }
    let px_per_tick = editor.editor_state.view.zoom_x;
    let px_per_key = editor.editor_state.view.zoom_y;
    match &shape.source {
        DrawnShapeSource::Shape {
            kind,
            rect,
            shift_constrained,
            ..
        } => {
            let rect = effective_rect(*kind, *rect, *shift_constrained, px_per_tick, px_per_key);
            if *kind == lumino_editor_state::ShapeKind::Circle {
                let (cx0, cy0, cx1, cy1) = rect;
                let mx = (cx0 + cx1) / 2.0;
                let my = (cy0 + cy1) / 2.0;
                let center = editor.line_pos_screen_pos((mx, my));
                let left = editor.line_pos_screen_pos((cx0, my));
                let right = editor.line_pos_screen_pos((cx1, my));
                let top = editor.line_pos_screen_pos((mx, cy1));
                let bottom = editor.line_pos_screen_pos((mx, cy0));
                let rx = (right.x - left.x).abs() * 0.5;
                let ry = (bottom.y - top.y).abs() * 0.5;
                Some((ellipse_path(center, rx, ry), true))
            } else {
                let verts = shape_vertices(*kind, rect, false, px_per_tick, px_per_key)?;
                let points: Vec<Point> = verts
                    .iter()
                    .map(|&(t, k)| editor.line_pos_screen_pos((t, k)))
                    .collect();
                Some((open_path(&points, true), true))
            }
        }
        DrawnShapeSource::Polyline { points } => {
            if points.len() < 2 {
                // 单点笔画：以短十字代替（避免空路径）
                let p = editor.line_pos_screen_pos(points.first().copied()?);
                let mark = Path::new(|b| {
                    b.move_to(Point::new(p.x - 4.0, p.y));
                    b.line_to(Point::new(p.x + 4.0, p.y));
                    b.move_to(Point::new(p.x, p.y - 4.0));
                    b.line_to(Point::new(p.x, p.y + 4.0));
                });
                return Some((mark, false));
            }
            let screen: Vec<Point> = points
                .iter()
                .map(|&(t, k)| editor.line_pos_screen_pos((t, k)))
                .collect();
            Some((open_path(&screen, false), false))
        }
    }
}

/// 连点成线；`close` = 首尾闭合
fn open_path(points: &[Point], close: bool) -> Path {
    Path::new(|p| {
        let Some(first) = points.first() else {
            return;
        };
        p.move_to(*first);
        for pt in points.iter().skip(1) {
            p.line_to(*pt);
        }
        if close {
            p.close();
        }
    })
}

/// 绘制选中图形的高亮描边（外发光 + 实线）
///
/// 无选中图形 / 选中图形不在当前音轨 / 已被撤销隐藏时返回 `None`。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let (path, _closed) = selected_outline(editor)?;
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let color = theme.extended_palette().primary.strong.color;

    let glow = Color::from_rgba(color.r, color.g, color.b, GLOW_ALPHA);
    frame.stroke(&path, Stroke::default().with_width(GLOW_WIDTH).with_color(glow));
    frame.stroke(
        &path,
        Stroke::default().with_width(CORE_WIDTH).with_color(color),
    );
    Some(frame.into_geometry())
}
