//! 已绘制图形叠加层：**常显轮廓**（鼠标工具）+ **选中高亮** + **框选矩形**
//!
//! 「鼠标工具」（`Tool::ShapeSelect`）需要「看得见才点得中」：
//! - **常显轮廓**：鼠标工具激活时，把当前音轨上全部可见图形以弱色细线画出，
//!   使用户一眼看到「哪些图案可选中」。否则切换工具后画布上只剩音符，
//!   图形轮廓不可见，表现为「图案消失了、无从框选」。
//! - **选中高亮**：被选中图形叠加发光描边（外发光 + 实线双描边），与
//!   `shape_tool_box` 的待确认预览区分（预览为细线，高亮更粗且带光晕）。
//! - **框选矩形**：空白处拉框时实时绘制半透明选择框。
//!
//! 渲染口径与命中测试同源：形状工具图形走 `shape_vertices` / 椭圆，折线
//! （曲线展平路径 / 画刷笔画）直接连点成线。仅在图形属于**当前音轨**且未被
//! 撤销隐藏时绘制。

use crate::Editor;
use iced_core::{Color, Point, Rectangle};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_editor_state::{DrawnShape, DrawnShapeSource};
use lumino_editor_state::shape_tool::{effective_rect, shape_vertices};
use lumino_ui_core::{Renderer, Theme};

/// 外发光描边宽度（像素）
const GLOW_WIDTH: f32 = 7.0;
/// 实线描边宽度（像素）
const CORE_WIDTH: f32 = 2.5;
/// 外发光透明度
const GLOW_ALPHA: f32 = 0.30;
/// 常显轮廓线宽（像素）
const IDLE_WIDTH: f32 = 1.5;
/// 常显轮廓透明度（弱化，避免盖过音符）
const IDLE_ALPHA: f32 = 0.55;
/// 框选矩形填充透明度
const MARQUEE_FILL_ALPHA: f32 = 0.12;
/// 框选矩形描边透明度
const MARQUEE_STROKE_ALPHA: f32 = 0.75;

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

/// 图形轮廓 → 屏幕坐标路径
///
/// `dtick` / `dkey` 为额外预览偏移（拖拽移动进行中时传拖拽偏移，仅渲染层位移、
/// 文档未改；常显轮廓与未拖拽的选中图形传 0）。
fn outline_path(editor: &Editor, shape: &DrawnShape, dtick: f32, dkey: f32) -> Option<Path> {
    let px_per_tick = editor.editor_state.view.zoom_x;
    let px_per_key = editor.editor_state.view.zoom_y;
    match &shape.source {
        DrawnShapeSource::Shape {
            kind,
            rect,
            shift_constrained,
            ..
        } => {
            let rect = (rect.0 + dtick, rect.1 + dkey, rect.2 + dtick, rect.3 + dkey);
            let rect = effective_rect(*kind, rect, *shift_constrained, px_per_tick, px_per_key);
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
                let verts = shape_vertices(*kind, rect, false, px_per_tick, px_per_key)?;
                let points: Vec<Point> = verts
                    .iter()
                    .map(|&(t, k)| editor.line_pos_screen_pos((t, k)))
                    .collect();
                Some(open_path(&points, true))
            }
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

/// 选中图形的轮廓（含拖拽实时预览偏移）
fn selected_outline(editor: &Editor) -> Option<Path> {
    let shape = editor.editor_state.shape_select.selected_shape()?;
    if shape.track != editor.editor_state.data.current_track {
        return None;
    }
    // 预览偏移：正在拖拽的正是当前选中图形时生效
    let (dtick, dkey) = match editor.editor_state.shape_select.drag() {
        Some(d) if d.shape_id == shape.id => (d.delta_tick, d.delta_key),
        _ => (0.0, 0.0),
    };
    outline_path(editor, shape, dtick, dkey)
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

/// 两点决定的轴对齐矩形路径（框选用）
fn rect_path(a: Point, b: Point) -> Path {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = (a.x.max(b.x), a.y.max(b.y));
    Path::new(|p| {
        p.move_to(Point::new(x0, y0));
        p.line_to(Point::new(x1, y0));
        p.line_to(Point::new(x1, y1));
        p.line_to(Point::new(x0, y1));
        p.close();
    })
}

/// 绘制已绘制图形叠加层（常显轮廓 / 选中高亮 / 框选矩形）
///
/// 三者皆无时返回 `None`（不产生空几何）。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let palette = theme.extended_palette();
    let strong = palette.primary.strong.color;
    let idle = Color::from_rgba(
        palette.background.strongest.color.r,
        palette.background.strongest.color.g,
        palette.background.strongest.color.b,
        IDLE_ALPHA,
    );
    let marquee_color = palette.primary.base.color;

    // 鼠标工具激活时把全部可见图形以弱色轮廓常显：没有它，「图案」在切换工具后
    // 就只剩音符、看不见可选中范围（用户反馈：切换后图案消失、无从框选）。
    // 其余工具只画选中高亮，避免常显轮廓干扰正常音符编辑。
    let select_mode = editor.current_tool() == lumino_message::Tool::ShapeSelect;
    let current_track = editor.editor_state.data.current_track;
    let selected_id = editor.editor_state.shape_select.selected();
    let idle_paths: Vec<Path> = if select_mode {
        editor
            .editor_state
            .shape_select
            .visible_on(current_track)
            .filter(|s| Some(s.id) != selected_id)
            .filter_map(|s| outline_path(editor, s, 0.0, 0.0))
            .collect()
    } else {
        Vec::new()
    };
    let selected_path = selected_outline(editor);
    let marquee_path = editor
        .editor_state
        .shape_select
        .marquee()
        .map(|m| {
            let (t0, t1, k0, k1) = m.rect();
            let a = editor.line_pos_screen_pos((t0, k0));
            let b = editor.line_pos_screen_pos((t1, k1));
            rect_path(a, b)
        });

    if idle_paths.is_empty() && selected_path.is_none() && marquee_path.is_none() {
        return None;
    }

    let mut frame = canvas::Frame::new(renderer, bounds.size());

    for path in &idle_paths {
        frame.stroke(
            path,
            Stroke::default().with_width(IDLE_WIDTH).with_color(idle),
        );
    }

    if let Some(path) = &selected_path {
        let glow = Color::from_rgba(strong.r, strong.g, strong.b, GLOW_ALPHA);
        frame.stroke(path, Stroke::default().with_width(GLOW_WIDTH).with_color(glow));
        frame.stroke(
            path,
            Stroke::default().with_width(CORE_WIDTH).with_color(strong),
        );
    }

    if let Some(path) = &marquee_path {
        let fill = Color::from_rgba(
            marquee_color.r,
            marquee_color.g,
            marquee_color.b,
            MARQUEE_FILL_ALPHA,
        );
        let stroke_color = Color::from_rgba(
            marquee_color.r,
            marquee_color.g,
            marquee_color.b,
            MARQUEE_STROKE_ALPHA,
        );
        frame.fill(path, fill);
        frame.stroke(
            path,
            Stroke::default().with_width(1.0).with_color(stroke_color),
        );
    }

    Some(frame.into_geometry())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;
    use lumino_editor_state::ShapeKind;

    /// 空编辑器 + 一个已登记的矩形图形（逻辑 0..4 × key 60..64）
    fn editor_with_rect() -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Shape {
                kind: ShapeKind::Rectangle,
                rect: (0.0, 60.0, 4.0, 64.0),
                shift_constrained: false,
                filled: false,
            },
            Vec::new(),
        );
        editor
    }

    fn polyline(points: Vec<(f32, f32)>) -> DrawnShape {
        DrawnShape {
            id: 0,
            track: 1,
            group: None,
            source: DrawnShapeSource::Polyline { points },
            notes: Vec::new(),
            hidden_by_creation: false,
            deleted: false,
            ever_deleted: false,
            moves: Vec::new(),
        }
    }

    #[test]
    fn test_outline_path_covers_shape_and_polyline_forms() {
        let editor = editor_with_rect();
        let shape = &editor.editor_state.shape_select.shapes()[0];
        assert!(
            outline_path(&editor, shape, 0.0, 0.0).is_some(),
            "形状工具图形应产生轮廓路径"
        );
        assert!(
            outline_path(&editor, &polyline(vec![(0.0, 60.0), (4.0, 64.0)]), 0.0, 0.0).is_some(),
            "折线应产生路径"
        );
        assert!(
            outline_path(&editor, &polyline(vec![(0.0, 60.0)]), 0.0, 0.0).is_some(),
            "单点笔画应退化为十字而非空路径"
        );
        assert!(
            outline_path(&editor, &polyline(Vec::new()), 0.0, 0.0).is_none(),
            "空折线不应产生路径"
        );
    }

    #[test]
    fn test_rect_path_accepts_reversed_corners() {
        // 反向两角 → 仍能规范化成矩形路径（仅构建，不断言像素）
        let _p = rect_path(Point::new(10.0, 20.0), Point::new(2.0, 5.0));
    }
}
