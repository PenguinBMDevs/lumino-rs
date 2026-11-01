//! 绘制图形轮廓 → 屏幕路径（从 `drawn_shape_box.rs` 拆出，控制单文件长度）

use iced_core::Point;
use iced_widget::canvas::Path;
use lumino_editor_state::shape_tool::{ShapeSpec, effective_rect, shape_vertices};
use lumino_editor_state::{DrawnShape, DrawnShapeSource};

use crate::Editor;

/// 生成椭圆路径（本 iced 版本无 `Path::ellipse`，改用多边形逼近）
pub(super) fn ellipse_path(center: Point, rx: f32, ry: f32) -> Path {
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

/// 图形轮廓 → 屏幕坐标路径
///
/// `dtick` / `dkey` 为额外预览偏移（拖拽移动进行中时传拖拽偏移，仅渲染层位移、
/// 文档未改；常显轮廓与未拖拽的选中图形传 0）。
pub(super) fn outline_path(
    editor: &Editor,
    shape: &DrawnShape,
    dtick: f32,
    dkey: f32,
) -> Option<Path> {
    let px_per_tick = editor.editor_state.view.zoom_x;
    let px_per_key = editor.editor_state.view.zoom_y;
    match &shape.source {
        DrawnShapeSource::Shape {
            kind,
            rect,
            shift_constrained,
            apex_high,
            ..
        } => {
            let rect = (rect.0 + dtick, rect.1 + dkey, rect.2 + dtick, rect.3 + dkey);
            let spec = ShapeSpec {
                kind: *kind,
                rect,
                shift_constrained: *shift_constrained,
                apex_high: *apex_high,
            };
            let rect = effective_rect(spec, px_per_tick, px_per_key);
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
                Some(ellipse_path(center, rx, ry))
            } else {
                let verts = shape_vertices(spec, px_per_tick, px_per_key)?;
                let points: Vec<Point> = verts
                    .iter()
                    .map(|&(t, k)| editor.line_pos_screen_pos((t, k)))
                    .collect();
                Some(open_path(&points, true))
            }
        }
        DrawnShapeSource::PolylineSet { polylines } => {
            let shifted = |&(t, k): &(f32, f32)| (t + dtick, k + dkey);
            let screen_sets: Vec<Vec<Point>> = polylines
                .iter()
                .filter(|points| !points.is_empty())
                .map(|points| {
                    points
                        .iter()
                        .map(|pt| editor.line_pos_screen_pos(shifted(pt)))
                        .collect()
                })
                .collect();
            if screen_sets.is_empty() {
                return None;
            }
            Some(Path::new(|p| {
                for points in &screen_sets {
                    if points.len() < 2 {
                        let pt = points[0];
                        p.move_to(Point::new(pt.x - 4.0, pt.y));
                        p.line_to(Point::new(pt.x + 4.0, pt.y));
                        p.move_to(Point::new(pt.x, pt.y - 4.0));
                        p.line_to(Point::new(pt.x, pt.y + 4.0));
                        continue;
                    }
                    p.move_to(points[0]);
                    for pt in &points[1..] {
                        p.line_to(*pt);
                    }
                }
            }))
        }
        DrawnShapeSource::Polyline { points } => {
            if points.is_empty() {
                return None;
            }
            let shifted = |&(t, k): &(f32, f32)| (t + dtick, k + dkey);
            if points.len() < 2 {
                // 单点笔画：以短十字代替（避免空路径）
                let p = editor.line_pos_screen_pos(shifted(&points[0]));
                let mark = Path::new(|b| {
                    b.move_to(Point::new(p.x - 4.0, p.y));
                    b.line_to(Point::new(p.x + 4.0, p.y));
                    b.move_to(Point::new(p.x, p.y - 4.0));
                    b.line_to(Point::new(p.x, p.y + 4.0));
                });
                return Some(mark);
            }
            let screen: Vec<Point> = points
                .iter()
                .map(|pt| editor.line_pos_screen_pos(shifted(pt)))
                .collect();
            Some(open_path(&screen, false))
        }
    }
}

/// 连点成线；`close` = 首尾闭合
pub(super) fn open_path(points: &[Point], close: bool) -> Path {
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
