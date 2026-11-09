//! 抽屉画布的绘制与单位框映射（从 canvas.rs 拆出，控制文件长度）。

use iced_core::{Point, Rectangle, Size};

use crate::shape_drawer::doc::{DrawerTool, DrawerView};

pub(crate) const TOOLBAR_WIDTH: f32 = 104.0;
pub(crate) const TOOLBAR_TOP: f32 = 16.0;
pub(crate) const ROW_HEIGHT: f32 = 30.0;
pub(crate) const ROW_GAP: f32 = 4.0;
pub(crate) const PADDING: f32 = 18.0;

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

pub(crate) fn unit_rect(bounds: Rectangle, view: DrawerView) -> Rectangle {
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

pub(crate) fn unit_side(bounds: Rectangle, view: DrawerView) -> f32 {
    let width = (bounds.width - TOOLBAR_WIDTH - PADDING * 2.0).max(1.0);
    let height = (bounds.height - PADDING * 2.0).max(1.0);
    width.min(height) * 0.85 * view.zoom.max(0.25)
}

pub(crate) fn unit_to_local(unit: (f32, f32), bounds: Rectangle, view: DrawerView) -> Point {
    let rect = unit_rect(bounds, view);
    Point::new(rect.x + rect.width * unit.0, rect.y + rect.height * unit.1)
}

pub(crate) fn local_to_unit(local: Point, bounds: Rectangle, view: DrawerView) -> (f32, f32) {
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
