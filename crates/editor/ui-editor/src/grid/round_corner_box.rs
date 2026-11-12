//! DRAW-044：角度柔化模式的可视化
//!
//! 画两样东西：柔化后的路径（与音符生成同一条 `rounded_points`）与角点手柄。
//! 选中角点用实心方块标出。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_core::shape::ShapeOp;
use lumino_ui_core::Renderer;

use crate::Editor;

/// 柔化路径颜色（青色）
const PATH_COLOUR: Color = Color {
    r: 0.20,
    g: 0.78,
    b: 0.86,
    a: 0.95,
};
/// 角点手柄颜色（橙色，与曲线工具的白色锚点区分）
const HANDLE_COLOUR: Color = Color {
    r: 1.0,
    g: 0.58,
    b: 0.12,
    a: 1.0,
};
/// 角点手柄边长（像素）
const HANDLE: f32 = 10.0;

/// 绘制柔化预览 + 角点手柄
pub fn draw(editor: &Editor, renderer: &Renderer, bounds: Rectangle) -> Option<Geometry<Renderer>> {
    let mode = editor.round_corner.as_ref()?;
    let model = editor
        .editor_state
        .pending_composed
        .get(mode.composed)?
        .results
        .first()?
        .model
        .clone();
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let anchors: Vec<(f64, f64)> = model
        .geometry
        .anchors
        .iter()
        .map(|anchor| (f64::from(anchor.pos.0), f64::from(anchor.pos.1)))
        .collect();
    let params = model.ops.iter().find_map(|op| match op {
        ShapeOp::Round(params) => Some(params),
        _ => None,
    });
    let path_points = match params {
        Some(params) => lumino_editor_state::shape_ops::rounded_points(
            &anchors,
            params,
            f64::from(params.effective_ppq(editor.editor_state.view.ppq)),
            f64::from(params.effective_k(model.k)),
        ),
        None => anchors.clone(),
    };
    if path_points.len() >= 2 {
        let screen: Vec<Point> = path_points
            .iter()
            .map(|(tick, key)| editor.line_pos_screen_pos((*tick as f32, *key as f32)))
            .collect();
        let path = Path::new(|builder| {
            builder.move_to(screen[0]);
            for point in screen.iter().skip(1) {
                builder.line_to(*point);
            }
        });
        frame.stroke(
            &path,
            Stroke::default().with_width(1.5).with_color(PATH_COLOUR),
        );
    }
    // 手柄与命中口径一致：只画「能真正柔化」的角点（端点/闭合重复点/共线点不画）
    let handle_points = editor.round_corner_points();
    for (index, center) in &handle_points {
        let center = *center;
        let rect = Rectangle::new(
            Point::new(center.x - HANDLE / 2.0, center.y - HANDLE / 2.0),
            Size::new(HANDLE, HANDLE),
        );
        let selected = mode.corner == Some(*index);
        let path = Path::rectangle(rect.position(), rect.size());
        if selected {
            frame.fill(&path, HANDLE_COLOUR);
            frame.stroke(
                &path,
                Stroke::default().with_width(1.5).with_color(Color::WHITE),
            );
        } else {
            frame.fill(&path, HANDLE_COLOUR);
            frame.stroke(
                &path,
                Stroke::default().with_width(1.0).with_color(PATH_COLOUR),
            );
        }
    }
    // 拖拽中：从角点拉一条提示线并显示当前半径，避免「拖了但看不出变化」
    if let (Some(corner), Some(cursor)) = (mode.corner, mode.cursor)
        && let Some((tick, key)) = anchors.get(corner)
    {
        let a = editor.line_pos_screen_pos((*tick as f32, *key as f32));
        let b = editor.line_pos_screen_pos((cursor.0, cursor.1));
        frame.stroke(
            &Path::line(a, b),
            Stroke::default().with_width(1.0).with_color(PATH_COLOUR),
        );
        if let Some(radius) = params.and_then(|params| params.radius_of(corner)) {
            frame.fill_text(canvas::Text {
                content: format!("r {radius:.1}"),
                position: Point::new(b.x + 8.0, b.y - 18.0),
                max_width: 80.0,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(12.0),
                color: PATH_COLOUR,
                font: lumino_ui_core::font::ui_font(),
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Advanced,
            });
        }
    }
    // 模式横幅：明确告诉用户「现在处于柔化模式、拖的是橙色角点」
    {
        let view = &editor.editor_state.view;
        let origin = Point::new(view.keyboard_width + 12.0, view.ruler_height + 12.0);
        let rect = Rectangle::new(origin, Size::new(330.0, 24.0));
        frame.fill(
            &Path::rounded_rectangle(
                rect.position(),
                rect.size(),
                iced_core::border::Radius::from(5.0),
            ),
            Color::from_rgba(0.0, 0.0, 0.0, 0.72),
        );
        let label = if handle_points.is_empty() {
            lumino_core::tr!("没有可柔化的角：先把中间节点拖出直线（Esc 退出）")
        } else {
            lumino_core::tr!("角度柔化 · 拖橙色角点调半径 · Esc 退出")
        };
        frame.fill_text(canvas::Text {
            content: label.to_string(),
            position: Point::new(origin.x + 8.0, origin.y + 4.0),
            max_width: rect.width - 16.0,
            line_height: iced_core::text::LineHeight::Relative(1.0),
            size: iced_core::Pixels(12.0),
            color: Color::WHITE,
            font: lumino_ui_core::font::ui_font(),
            align_x: iced_core::alignment::Horizontal::Left.into(),
            align_y: iced_core::alignment::Vertical::Top,
            shaping: iced_core::text::Shaping::Advanced,
        });
    }
    Some(frame.into_geometry())
}
