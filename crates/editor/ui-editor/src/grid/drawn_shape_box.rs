//! 已绘制图形叠加层：**常显轮廓**（鼠标工具）+ **选中高亮** + **选中选框** + **框选矩形**
//!
//! 「鼠标工具」（`Tool::ShapeSelect`）需要「看得见才点得中」：
//! - **常显轮廓**：鼠标工具激活时，把当前音轨上全部可见图形以弱色细线画出，
//!   使用户一眼看到「哪些图案可选中」。否则切换工具后画布上只剩音符，
//!   图形轮廓不可见，表现为「图案消失了、无从框选」。
//! - **选中高亮**：被选中的每一个图形都叠加发光描边（外发光 + 实线双描边），与
//!   `shape_tool_box` 的待确认预览区分（预览为细线，高亮更粗且带光晕）。
//! - **选中选框**：选中集（多选时为**并集**）的屏幕外接框 + 四角手柄，把「当前选中了哪些
//!   图形、从哪里可以拖动它们」明示出来。框内区域就是可拖动区（命中判定见
//!   `interaction::drawn_shape` 的 `point_in_selection_box`）——渲染与命中同源。
//! - **框选矩形**：空白处拉框时实时绘制半透明选择框。
//!
//! 渲染口径与命中测试同源：形状工具图形走 `shape_vertices` / 椭圆，折线
//! （曲线展平路径 / 画刷笔画）直接连点成线。仅在图形属于**当前音轨**且未被
//! 撤销隐藏时绘制。

use crate::Editor;
use iced_core::{Color, Point, Rectangle, Size};
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
/// 选中选框描边宽度（像素）
const BOX_STROKE_WIDTH: f32 = 1.5;
/// 选中选框四角手柄边长（像素）
const BOX_HANDLE_SIZE: f32 = 6.0;
/// 选中选框描边透明度
const BOX_STROKE_ALPHA: f32 = 0.85;

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

/// 选中集内**全部**图形的轮廓（同轨；含整组拖拽实时预览偏移）
///
/// 多选时逐个选中图形都画发光描边；拖拽作用于整个选中集，故所有图形套用同一预览偏移。
fn selected_outlines(editor: &Editor) -> Vec<Path> {
    let track = editor.editor_state.data.current_track;
    let (dtick, dkey) = editor.drag_preview_delta();
    editor
        .editor_state
        .shape_select
        .selected_shapes_on(track)
        .filter_map(|s| outline_path(editor, s, dtick, dkey))
        .collect()
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

/// 叠加层绘制内容（与渲染后端解耦，便于单测「要不要画、画了什么」）
struct Overlay {
    /// 常显轮廓（鼠标工具下当前轨全部可见图形，排除已选中者）
    idle_paths: Vec<Path>,
    /// 选中高亮（逐个选中图形：发光 + 实线双描边）
    selected_paths: Vec<Path>,
    /// 选中集选框（并集外接框 + 四角手柄）
    selection_box: Option<Rectangle>,
    /// 框选拉框矩形
    marquee_path: Option<Path>,
}

impl Overlay {
    fn collect(editor: &Editor) -> Self {
        // 鼠标工具激活时把全部可见图形以弱色轮廓常显：没有它，「图案」在切换工具后
        // 就只剩音符、看不见可选中范围（用户反馈：切换后图案消失、无从框选）。
        // 其余工具只画选中高亮，避免常显轮廓干扰正常音符编辑。
        let select_mode = editor.current_tool() == lumino_message::Tool::ShapeSelect;
        let current_track = editor.editor_state.data.current_track;
        let idle_paths: Vec<Path> = if select_mode {
            editor
                .editor_state
                .shape_select
                .visible_on(current_track)
                .filter(|s| !editor.editor_state.shape_select.is_selected(s.id))
                .filter_map(|s| outline_path(editor, s, 0.0, 0.0))
                .collect()
        } else {
            Vec::new()
        };
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
        Self {
            idle_paths,
            selected_paths: selected_outlines(editor),
            // 选中选框：与命中判定同源（`Editor::selection_box`），
            // 保证「看到的框」就是「抓得住的框」。
            selection_box: editor.selection_box(),
            marquee_path,
        }
    }

    /// 四者皆无 → 不产生空几何
    fn is_empty(&self) -> bool {
        self.idle_paths.is_empty()
            && self.selected_paths.is_empty()
            && self.selection_box.is_none()
            && self.marquee_path.is_none()
    }
}

/// 绘制已绘制图形叠加层（常显轮廓 / 选中高亮 / 选中选框 / 框选矩形）
///
/// 四者皆无时返回 `None`（不产生空几何）。
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

    let overlay = Overlay::collect(editor);
    if overlay.is_empty() {
        return None;
    }

    let mut frame = canvas::Frame::new(renderer, bounds.size());

    for path in &overlay.idle_paths {
        frame.stroke(
            path,
            Stroke::default().with_width(IDLE_WIDTH).with_color(idle),
        );
    }

    if !overlay.selected_paths.is_empty() {
        let glow = Color::from_rgba(strong.r, strong.g, strong.b, GLOW_ALPHA);
        for path in &overlay.selected_paths {
            frame.stroke(path, Stroke::default().with_width(GLOW_WIDTH).with_color(glow));
            frame.stroke(
                path,
                Stroke::default().with_width(CORE_WIDTH).with_color(strong),
            );
        }
    }

    // 选中选框：外侧描边 + 四角实心手柄（手柄压在框角上，便于识别「可拖动」）
    if let Some(rect) = overlay.selection_box {
        let box_color = Color::from_rgba(strong.r, strong.g, strong.b, BOX_STROKE_ALPHA);
        let path = Path::rectangle(rect.position(), rect.size());
        frame.stroke(
            &path,
            Stroke::default()
                .with_width(BOX_STROKE_WIDTH)
                .with_color(box_color),
        );
        let half = BOX_HANDLE_SIZE / 2.0;
        let corners = [
            rect.position(),
            Point::new(rect.position().x + rect.width, rect.position().y),
            Point::new(rect.position().x, rect.position().y + rect.height),
            Point::new(
                rect.position().x + rect.width,
                rect.position().y + rect.height,
            ),
        ];
        for c in corners {
            let handle = Path::rectangle(
                Point::new(c.x - half, c.y - half),
                Size::new(BOX_HANDLE_SIZE, BOX_HANDLE_SIZE),
            );
            frame.fill(&handle, strong);
        }
    }

    if let Some(path) = &overlay.marquee_path {
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
    use crate::tests::test_helpers::seed_notes;
    use lumino_editor_state::ShapeKind;

    /// 非 Conductor 轨（track 1）+ 一个已登记的矩形图形（逻辑 0..4 × key 60..64）
    fn editor_with_rect() -> Editor {
        let mut editor = Editor::new();
        seed_notes(&mut editor, 2, 1, &[]);
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

    #[test]
    fn test_overlay_includes_selection_box_only_when_selected() {
        let mut editor = editor_with_rect();
        // 未选中且非鼠标工具 → 无内容，不产出几何
        let overlay = Overlay::collect(&editor);
        assert!(overlay.selection_box.is_none(), "未选中不应有选框");
        assert!(overlay.selected_paths.is_empty());
        assert!(overlay.is_empty(), "无内容时不应产出几何");

        // 选中后 → 有选框，叠加层非空（渲染侧不再返回 None）
        let id = editor.editor_state.shape_select.shapes()[0].id;
        editor.editor_state.shape_select.select_only(id);
        let overlay = Overlay::collect(&editor);
        let rect = overlay.selection_box.expect("选中后应有选框");
        assert!(rect.width > 0.0 && rect.height > 0.0, "选框必须有面积");
        assert_eq!(overlay.selected_paths.len(), 1, "选中高亮仍应保留");
        assert!(!overlay.is_empty(), "有选框时应产出几何");
    }

    #[test]
    fn test_mouse_tool_shows_idle_outlines_and_marquee() {
        use lumino_message::Tool;
        let mut editor = editor_with_rect();
        // 再加一个图形，验证「常显轮廓排除已选中者」：被选中者单独走高亮
        editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Shape {
                kind: ShapeKind::Rectangle,
                rect: (8.0, 60.0, 12.0, 64.0),
                shift_constrained: false,
                filled: false,
            },
            Vec::new(),
        );
        editor.set_tool(Tool::ShapeSelect);
        let first = editor.editor_state.shape_select.shapes()[0].id;
        editor.editor_state.shape_select.select_only(first);
        editor.editor_state.shape_select.begin_marquee(0.0, 60.0, false);
        editor.editor_state.shape_select.update_marquee(20.0, 70.0);

        let overlay = Overlay::collect(&editor);
        assert_eq!(
            overlay.idle_paths.len(),
            1,
            "常显轮廓应排除已选中的那一个（它走高亮）"
        );
        assert_eq!(overlay.selected_paths.len(), 1);
        assert!(overlay.marquee_path.is_some(), "拉框中应画出拉框矩形");
        assert!(overlay.selection_box.is_some());
    }

    #[test]
    fn test_overlay_glows_every_selected_shape_and_boxes_union() {
        use lumino_message::Tool;
        let mut editor = editor_with_rect();
        // 第二个图形（并列在右下），用于验证「多选 → 逐个高亮 + 选框取并集」
        let second = editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Shape {
                kind: ShapeKind::Rectangle,
                rect: (20.0, 40.0, 24.0, 44.0),
                shift_constrained: false,
                filled: false,
            },
            Vec::new(),
        );
        editor.set_tool(Tool::ShapeSelect);
        let first = editor.editor_state.shape_select.shapes()[0].id;
        editor.editor_state.shape_select.select_all_of([first, second]);

        let overlay = Overlay::collect(&editor);
        assert_eq!(
            overlay.selected_paths.len(),
            2,
            "多选时每个选中图形都应有发光描边"
        );
        assert_eq!(
            overlay.idle_paths.len(),
            0,
            "全部被选中 → 无常显轮廓"
        );
        let rect = overlay.selection_box.expect("多选应有并集选框");
        // 并集外接框应同时包住两个图形的角点
        for c in [
            editor.line_pos_screen_pos((0.0, 60.0)),
            editor.line_pos_screen_pos((4.0, 64.0)),
            editor.line_pos_screen_pos((20.0, 40.0)),
            editor.line_pos_screen_pos((24.0, 44.0)),
        ] {
            assert!(
                rect.contains(c),
                "并集选框应包住各选中图形角点 {c:?}，实际 {rect:?}"
            );
        }
    }

    #[test]
    fn test_selection_box_encloses_shape_screen_bounds() {
        let mut editor = editor_with_rect();
        let id = editor.editor_state.shape_select.shapes()[0].id;
        editor.editor_state.shape_select.select_only(id);
        let rect = editor.selection_box().expect("应有选框");
        // 图形逻辑 0..4 × 60..64 的屏幕两角都应落在选框内部（选框只增不减）
        let corners = [
            editor.line_pos_screen_pos((0.0, 60.0)),
            editor.line_pos_screen_pos((4.0, 64.0)),
        ];
        for c in corners {
            assert!(
                rect.contains(c),
                "选框应包住图形屏幕角点 {c:?}，实际 {rect:?}"
            );
        }
    }
}
