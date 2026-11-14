//! 020「编辑图形」锚点布局（屏幕坐标）。
//!
//! 从 `shape_edit_ui.rs` 拆出，保持单文件 <400 行。

use lumino_editor_state::editor_state::{HandleKind, Zoom, box_handles};
use lumino_ui_core::geom::Point;

use crate::Editor;

/// 编辑态锚点屏幕位置：8 个箱体锚点 + 可选中心锚点。
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ShapeEditHandles {
    /// 4 角 + 4 边中点
    pub box_points: Vec<Point>,
    /// 中心锚点（仅未 Bake 组合）
    pub center: Option<Point>,
}

impl Editor {
    /// 编辑态锚点（屏幕坐标）：8 个箱体锚点 + 可选中心锚点。
    pub fn shape_edit_handles(&self) -> ShapeEditHandles {
        let Some((target, model)) = self.armed_edit_target() else {
            return ShapeEditHandles::default();
        };
        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let include_center = self.edit_allows_center(target);
        let Some(layout) = box_handles(&model, zoom, include_center) else {
            return ShapeEditHandles::default();
        };
        let (center, box_kinds): (Vec<_>, Vec<_>) = layout
            .handles
            .into_iter()
            .partition(|(kind, _)| matches!(kind, HandleKind::Move));
        ShapeEditHandles {
            box_points: box_kinds
                .into_iter()
                .map(|(_, pos)| self.line_pos_screen_pos(pos))
                .collect(),
            center: center
                .first()
                .map(|(_, pos)| self.line_pos_screen_pos(*pos)),
        }
    }
}
