//! 020 手柄命中/旋转区/光标反馈。
//!
//! 从 `shape_edit_ui.rs` 拆出，保持单文件 <400 行。

use iced_core::{Point, Rectangle, Size, mouse};
use lumino_core::shape::{ShapeKind, ShapeModel};
use lumino_editor_state::PendingShapeRef;
use lumino_editor_state::editor_state::{Corner, HandleKind, Side, Zoom, hit_test};

use crate::Editor;

impl Editor {
    /// 屏幕空间的选择框（模型几何包围盒，不做 padding）
    pub(super) fn shape_bounds_screen(&self, model: &ShapeModel) -> Option<Rectangle> {
        let (x0, y0, x1, y1) = model.geometry.bounds()?;
        let a = self.line_pos_screen_pos((x0, y0));
        let b = self.line_pos_screen_pos((x1, y1));
        Some(Rectangle::new(
            Point::new(a.x.min(b.x), a.y.min(b.y)),
            Size::new((b.x - a.x).abs(), (b.y - a.y).abs()),
        ))
    }

    /// 箱体手柄只对足够大的选择框开放；极薄/极小矩形交给整组拖动。
    /// 线/折线/曲线的锚点手柄不受尺寸限制。
    pub(super) fn box_handles_enabled(&self, model: &ShapeModel) -> bool {
        if matches!(
            model.kind,
            ShapeKind::Line | ShapeKind::Polyline | ShapeKind::Freehand | ShapeKind::Curve
        ) {
            return true;
        }
        self.shape_bounds_screen(model)
            .is_some_and(|rect| rect.width >= 24.0 && rect.height >= 24.0)
    }

    pub(super) fn handle_allowed(&self, model: &ShapeModel, handle: HandleKind) -> bool {
        matches!(handle, HandleKind::AnchorHandle(_) | HandleKind::Path)
            || self.box_handles_enabled(model)
    }

    pub(super) fn point_near_rect(rect: Rectangle, point: Point, pad: f32) -> bool {
        point.x >= rect.x - pad
            && point.x <= rect.x + rect.width + pad
            && point.y >= rect.y - pad
            && point.y <= rect.y + rect.height + pad
    }

    /// 角外旋转区：选择框四角外侧 32px 的对角区域。
    pub(super) fn in_shape_rotate_zone(&self, model: &ShapeModel, logical: (f32, f32)) -> bool {
        let point = self.line_pos_screen_pos(logical);
        let Some(rect) = self.shape_bounds_screen(model) else {
            return false;
        };
        let right = rect.x + rect.width;
        let bottom = rect.y + rect.height;
        let dx = if point.x < rect.x {
            rect.x - point.x
        } else if point.x > right {
            point.x - right
        } else {
            0.0
        };
        let dy = if point.y < rect.y {
            rect.y - point.y
        } else if point.y > bottom {
            point.y - bottom
        } else {
            0.0
        };
        dx > 0.0 && dy > 0.0 && dx <= 32.0 && dy <= 32.0
    }

    /// ShapeSelect 悬停光标：移动/轴向拉伸/对角缩放/角外旋转。
    pub(crate) fn shape_handle_interaction(&self, pos: Point) -> Option<mouse::Interaction> {
        if self.current_tool() != lumino_message::Tool::ShapeSelect
            || self.editor_state.shape_select.selection_len() != 1
        {
            return None;
        }
        let drawn = self.editor_state.shape_select.selected_shapes().next()?;
        let model = match drawn.pending {
            Some(PendingShapeRef::Composed(index)) => {
                let composed = self.editor_state.pending_composed.get(index)?;
                if !Self::pending_handle_supported(composed) {
                    return None;
                }
                composed.results.first().map(|part| &part.model)?
            }
            Some(_) => return None,
            None => {
                let model_id = drawn.model_id?;
                self.editor_state.shape_store.model(model_id)?
            }
        };
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos);
        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let inside_box = self
            .shape_bounds_screen(model)
            .is_some_and(|rect| Self::point_near_rect(rect, pos, 2.0));
        if let Some(handle) = hit_test(model, (tick, key), zoom, 10.0)
            && self.handle_allowed(model, handle)
            && (inside_box || matches!(handle, HandleKind::AnchorHandle(_) | HandleKind::Path))
        {
            return Some(match handle {
                HandleKind::ResizeCorner(Corner::Nw | Corner::Se) => {
                    mouse::Interaction::ResizingDiagonallyDown
                }
                HandleKind::ResizeCorner(Corner::Ne | Corner::Sw) => {
                    mouse::Interaction::ResizingDiagonallyUp
                }
                HandleKind::Side(Side::Left | Side::Right) => {
                    mouse::Interaction::ResizingHorizontally
                }
                HandleKind::Side(Side::Top | Side::Bottom) => {
                    mouse::Interaction::ResizingVertically
                }
                HandleKind::AnchorHandle(_) | HandleKind::Path => mouse::Interaction::Grab,
                _ => mouse::Interaction::Grab,
            });
        }
        if self.box_handles_enabled(model) && self.in_shape_rotate_zone(model, (tick, key)) {
            return Some(mouse::Interaction::Grab);
        }
        let rect = self.shape_bounds_screen(model)?;
        rect.contains(pos).then_some(mouse::Interaction::Move)
    }
}
