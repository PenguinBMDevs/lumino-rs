//! 形状列表（只读）：生产状态 → egui 模型。
//!
//! 几何来自 `shape_list_box::shape_list_rects`（与命中测试同源），
//! 这里只做画布局部坐标 → 窗口逻辑坐标的换算。

use lumino_core::tr;
use lumino_ui_editor::grid::shape_list_box;
use lumino_ui_egui::shape_list::{ShapeListModel, ShapeListRow};

use crate::root::Root;

impl Root {
    /// 本帧形状列表（空模型 = 不显示）。
    pub fn shape_list_model(&self) -> ShapeListModel {
        let editor = &self.editor;
        let Some(rects) = shape_list_box::shape_list_rects(editor) else {
            return ShapeListModel::default();
        };
        let canvas = &editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let to_rect = |rect: lumino_ui_core::geom::Rectangle| {
            egui::Rect::from_min_size(
                egui::pos2(rect.x + offset.x, rect.y + offset.y),
                egui::vec2(rect.width, rect.height),
            )
        };

        ShapeListModel {
            panel: to_rect(rects.panel),
            title: tr!("形状列表（只读）").to_string(),
            rows: rects
                .rows
                .iter()
                .map(|(_, rect, label)| ShapeListRow {
                    rect: to_rect(*rect),
                    label: label.clone(),
                })
                .collect(),
            close: to_rect(rects.close),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::storage::config::UiConfig;

    #[test]
    fn test_closed_list_has_empty_model() {
        let root = Root::new(&UiConfig::default());
        assert!(root.shape_list_model().is_empty());
    }

    #[test]
    fn test_open_list_with_baked_model_has_row() {
        let mut root = Root::new(&UiConfig::default());
        root.editor.editor_state.data.current_track = 1;
        root.editor.editor_state.canvas.size_x = 1000.0;
        root.editor.editor_state.canvas.size_y = 600.0;
        let mut model =
            lumino_core::shape::ShapeModel::new(0, 1, lumino_core::shape::ShapeKind::Polyline, 1.0);
        model.name = "test".to_string();
        root.editor.editor_state.shape_store.insert(model, 0);

        assert!(root.editor.open_shape_list(), "应能打开形状列表");
        let list = root.shape_list_model();
        assert!(!list.is_empty(), "列表模型应有行");
        assert_eq!(list.rows.len(), 1);
        assert_eq!(list.rows[0].label, "test");
    }
}
