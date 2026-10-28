//! 抽屉画布的绘制与单位框映射（从 canvas.rs 拆出，控制文件长度）。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Frame, Text, path};

use crate::shape_drawer::doc::{DrawerDoc, DrawerTool, DrawerView};
use crate::{Renderer, Theme};

pub(super) const TOOLBAR_WIDTH: f32 = 104.0;
pub(super) const TOOLBAR_TOP: f32 = 16.0;
pub(super) const ROW_HEIGHT: f32 = 30.0;
pub(super) const ROW_GAP: f32 = 4.0;
pub(super) const PADDING: f32 = 18.0;

#[derive(Debug, Clone, Copy)]
pub(super) enum ToolbarAction {
    Tool(DrawerTool),
    Undo,
    Redo,
    Delete,
}

pub(super) fn toolbar_hit(local: Point) -> Option<ToolbarAction> {
    let tools = DrawerTool::ALL;
    for (index, tool) in tools.into_iter().enumerate() {
        let top = TOOLBAR_TOP + index as f32 * (ROW_HEIGHT + ROW_GAP);
        if local.y >= top
            && local.y <= top + ROW_HEIGHT
            && local.x >= 8.0
            && local.x <= TOOLBAR_WIDTH - 8.0
        {
            return Some(ToolbarAction::Tool(tool));
        }
    }
    let base = TOOLBAR_TOP + tools.len() as f32 * (ROW_HEIGHT + ROW_GAP) + 12.0;
    for (index, action) in [
        ToolbarAction::Undo,
        ToolbarAction::Redo,
        ToolbarAction::Delete,
    ]
    .into_iter()
    .enumerate()
    {
        let top = base + index as f32 * (ROW_HEIGHT + ROW_GAP);
        if local.y >= top
            && local.y <= top + ROW_HEIGHT
            && local.x >= 8.0
            && local.x <= TOOLBAR_WIDTH - 8.0
        {
            return Some(action);
        }
    }
    None
}

pub(super) fn draw_toolbar(frame: &mut Frame<Renderer>, doc: &DrawerDoc, theme: &Theme) {
    let palette = theme.extended_palette();
    let selected = palette.primary.weak.color;
    let selected_text = palette.primary.weak.text;
    let button = palette.background.strong.color;
    let text = palette.background.strong.text;
    let disabled_text = palette.background.weak.text;
    let tools = DrawerTool::ALL;
    for (index, tool) in tools.into_iter().enumerate() {
        let top = TOOLBAR_TOP + index as f32 * (ROW_HEIGHT + ROW_GAP);
        let rect = Rectangle::new(
            Point::new(8.0, top),
            Size::new(TOOLBAR_WIDTH - 16.0, ROW_HEIGHT),
        );
        frame.fill_rectangle(
            rect.position(),
            rect.size(),
            if doc.tool == tool { selected } else { button },
        );
        frame.fill_text(Text {
            content: tool.label().to_string(),
            position: Point::new(16.0, top + 7.0),
            color: if doc.tool == tool {
                selected_text
            } else {
                text
            },
            size: 12.0.into(),
            ..Text::default()
        });
    }
    let base = TOOLBAR_TOP + tools.len() as f32 * (ROW_HEIGHT + ROW_GAP) + 12.0;
    for (index, (label, enabled)) in [
        ("Undo", !doc.undo_empty()),
        ("Redo", !doc.redo_empty()),
        ("Del", !doc.selection.is_empty()),
    ]
    .into_iter()
    .enumerate()
    {
        let top = base + index as f32 * (ROW_HEIGHT + ROW_GAP);
        let rect = Rectangle::new(
            Point::new(8.0, top),
            Size::new(TOOLBAR_WIDTH - 16.0, ROW_HEIGHT),
        );
        frame.fill_rectangle(
            rect.position(),
            rect.size(),
            if enabled {
                button
            } else {
                Color::from_rgba(0.0, 0.0, 0.0, 0.12)
            },
        );
        frame.fill_text(Text {
            content: label.to_string(),
            position: Point::new(16.0, top + 7.0),
            color: if enabled { text } else { disabled_text },
            size: 12.0.into(),
            ..Text::default()
        });
    }
}

pub(super) fn draw_canvas(
    frame: &mut Frame<Renderer>,
    doc: &DrawerDoc,
    bounds: Rectangle,
    theme: &Theme,
    shapes_dir: &str,
) {
    let palette = theme.extended_palette();
    let text = palette.background.base.text;
    let unit = unit_rect(bounds, doc.view);
    let grid = Color::from_rgba(0.5, 0.5, 0.5, 0.25);
    for step in 0..=4 {
        let t = step as f32 / 4.0;
        let x = unit.x + unit.width * t;
        let y = unit.y + unit.height * t;
        let mut vertical = path::Builder::new();
        vertical.move_to(Point::new(x, unit.y));
        vertical.line_to(Point::new(x, unit.y + unit.height));
        frame.stroke(
            &vertical.build(),
            canvas::Stroke::default().with_color(grid).with_width(1.0),
        );
        let mut horizontal = path::Builder::new();
        horizontal.move_to(Point::new(unit.x, y));
        horizontal.line_to(Point::new(unit.x + unit.width, y));
        frame.stroke(
            &horizontal.build(),
            canvas::Stroke::default().with_color(grid).with_width(1.0),
        );
    }
    frame.stroke(
        &path::Path::rectangle(unit.position(), unit.size()),
        canvas::Stroke::default().with_color(text).with_width(1.0),
    );
    for (index, stroke) in doc.strokes.iter().enumerate() {
        draw_stroke(
            frame,
            stroke,
            bounds,
            doc.view,
            text,
            doc.is_selected(index),
        );
    }
    if doc.strokes.is_empty() {
        frame.fill_text(Text {
            content: "Draw a stroke, or pick a tool on the left".to_string(),
            position: Point::new(unit.x + 16.0, unit.y + 16.0),
            color: palette.background.strong.text,
            size: 13.0.into(),
            ..Text::default()
        });
    }
    frame.fill_text(Text {
        content: format!("Library: {shapes_dir}"),
        position: Point::new(TOOLBAR_WIDTH + 12.0, bounds.height - 24.0),
        color: palette.background.strong.text,
        size: 11.0.into(),
        max_width: (bounds.width - TOOLBAR_WIDTH - 24.0).max(1.0),
        ..Text::default()
    });
}

pub(super) fn draw_stroke(
    frame: &mut Frame<Renderer>,
    stroke: &lumino_core::shape::Stroke,
    bounds: Rectangle,
    view: DrawerView,
    color: Color,
    selected: bool,
) {
    if stroke.anchors.len() < 2 {
        return;
    }
    let mut builder = path::Builder::new();
    let first = unit_to_local(stroke.anchors[0].pos, bounds, view);
    builder.move_to(first);
    if stroke.kind == lumino_core::shape::StrokeKind::Curve {
        for window in stroke.anchors.windows(2) {
            let a = window[0];
            let b = window[1];
            builder.bezier_curve_to(
                unit_to_local(a.out_handle_abs(), bounds, view),
                unit_to_local(b.in_handle_abs(), bounds, view),
                unit_to_local(b.pos, bounds, view),
            );
        }
    } else {
        for anchor in &stroke.anchors[1..] {
            builder.line_to(unit_to_local(anchor.pos, bounds, view));
        }
    }
    frame.stroke(
        &builder.build(),
        canvas::Stroke::default()
            .with_color(if selected {
                Color::from_rgb(1.0, 0.72, 0.2)
            } else {
                color
            })
            .with_width(if selected { 3.0 } else { 2.0 }),
    );
}

pub(super) fn unit_rect(bounds: Rectangle, view: DrawerView) -> Rectangle {
    let side = unit_side(bounds, view);
    let area = Rectangle::new(
        Point::new(TOOLBAR_WIDTH + PADDING, PADDING),
        Size::new(
            (bounds.width - TOOLBAR_WIDTH - PADDING * 2.0).max(1.0),
            (bounds.height - PADDING * 2.0).max(1.0),
        ),
    );
    let center = Point::new(area.x + area.width * 0.5, area.y + area.height * 0.5);
    let cx = center.x + view.pan.0 * side;
    let cy = center.y + view.pan.1 * side;
    Rectangle::new(
        Point::new(cx - side * 0.5, cy - side * 0.5),
        Size::new(side, side),
    )
}

pub(super) fn unit_side(bounds: Rectangle, view: DrawerView) -> f32 {
    let width = (bounds.width - TOOLBAR_WIDTH - PADDING * 2.0).max(1.0);
    let height = (bounds.height - PADDING * 2.0).max(1.0);
    width.min(height) * 0.85 * view.zoom.max(0.25)
}

fn unit_to_local(unit: (f32, f32), bounds: Rectangle, view: DrawerView) -> Point {
    let rect = unit_rect(bounds, view);
    Point::new(rect.x + rect.width * unit.0, rect.y + rect.height * unit.1)
}

pub(super) fn local_to_unit(local: Point, bounds: Rectangle, view: DrawerView) -> (f32, f32) {
    let rect = unit_rect(bounds, view);
    let u = ((local.x - rect.x) / rect.width).clamp(0.0, 1.0);
    let v = ((local.y - rect.y) / rect.height).clamp(0.0, 1.0);
    (u, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Rectangle {
        Rectangle::new(Point::new(0.0, 0.0), Size::new(1000.0, 700.0))
    }

    #[test]
    fn test_unit_to_local_roundtrip() {
        let view = DrawerView {
            zoom: 1.0,
            pan: (0.2, -0.1),
        };
        for unit in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)] {
            let local = unit_to_local(unit, bounds(), view);
            let back = local_to_unit(local, bounds(), view);
            assert!((back.0 - unit.0).abs() < 1e-4);
            assert!((back.1 - unit.1).abs() < 1e-4);
        }
    }

    #[test]
    fn test_toolbar_hit_returns_tools_and_actions() {
        assert!(matches!(
            toolbar_hit(Point::new(20.0, TOOLBAR_TOP + 5.0)),
            Some(ToolbarAction::Tool(DrawerTool::Pen))
        ));
        let base = TOOLBAR_TOP + DrawerTool::ALL.len() as f32 * (ROW_HEIGHT + ROW_GAP) + 12.0;
        assert!(matches!(
            toolbar_hit(Point::new(20.0, base + 5.0)),
            Some(ToolbarAction::Undo)
        ));
    }
}
