//! 020 「编辑图形」手柄命中/旋转区/光标反馈。
//!
//! 从 `shape_edit_ui.rs` 拆出，保持单文件 <400 行。

use iced_core::{Point, Rectangle, Size, mouse};
use lumino_core::shape::ShapeModel;
use lumino_editor_state::PendingShapeRef;
use lumino_editor_state::editor_state::{Corner, HandleKind, Side, Zoom, hit_test_box};

use crate::Editor;

use super::ShapeEditTarget;

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

    /// 当前「编辑图形」目标与其模型；目标不匹配或不可编辑 → `None`。
    ///
    /// 渲染（8+1 锚点/编辑框）、命中、光标都从这里取同一份目标与模型，
    /// 避免「看得见的锚点」与「点得中的锚点」分叉。
    pub(super) fn armed_edit_target(&self) -> Option<(ShapeEditTarget, &ShapeModel)> {
        let armed = self.shape_edit_armed?;
        if self.editor_state.shape_select.selection_len() != 1 {
            return None;
        }
        let drawn = self.editor_state.shape_select.selected_shapes().next()?;
        let (target, model) = match drawn.pending {
            Some(PendingShapeRef::Composed(index)) => {
                let composed = self.editor_state.pending_composed.get(index)?;
                if !Self::pending_handle_supported(composed) {
                    return None;
                }
                (
                    ShapeEditTarget::PendingComposed { index },
                    &composed.results.first()?.model,
                )
            }
            Some(_) => return None,
            None => {
                let model_id = drawn.model_id?;
                (
                    ShapeEditTarget::Drawn {
                        drawn_id: drawn.id,
                        model_id,
                    },
                    self.editor_state.shape_store.model(model_id)?,
                )
            }
        };
        (target == armed).then_some((target, model))
    }

    /// 编辑目标是否允许中心锚点：只有未 Bake 的单支组合可整体平移。
    pub(super) fn edit_allows_center(&self, target: ShapeEditTarget) -> bool {
        matches!(target, ShapeEditTarget::PendingComposed { .. })
    }

    /// 编辑框（屏幕矩形）；未进入「编辑图形」→ `None`。
    pub(crate) fn shape_edit_box(&self) -> Option<Rectangle> {
        let (_, model) = self.armed_edit_target()?;
        self.shape_bounds_screen(model)
    }

    /// 角外旋转区：编辑框四角外侧 32px 的对角区域。
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

    /// 编辑态悬停光标：8+1 锚点 + 角外旋转。
    ///
    /// 未进入「编辑图形」时返回 `None`，由普通 ShapeSelect 光标接管。
    pub(crate) fn shape_handle_interaction(&self, pos: Point) -> Option<mouse::Interaction> {
        let (target, model) = self.armed_edit_target()?;
        if self.current_tool() != lumino_message::Tool::ShapeSelect {
            return None;
        }
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos);
        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let include_center = self.edit_allows_center(target);
        if let Some(handle) = hit_test_box(model, (tick, key), zoom, 10.0, include_center) {
            return Some(match handle {
                HandleKind::Move => mouse::Interaction::Move,
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
                _ => mouse::Interaction::Grab,
            });
        }
        if self.in_shape_rotate_zone(model, (tick, key)) {
            return Some(mouse::Interaction::Grab);
        }
        None
    }
}
