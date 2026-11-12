//! 图片转 MIDI 区域框：生产几何 → egui 模型。

use lumino_editor_state::ImageToMidiMode;
use lumino_ui_egui::image_to_midi::I2mRegionModel;

use crate::root::Root;

impl Root {
    /// 本帧 I2M 区域框（仅 Placing 阶段显示）。
    pub fn i2m_region_model(&self) -> I2mRegionModel {
        let editor = &self.editor;
        if editor.editor_state.image_to_midi.mode != ImageToMidiMode::Placing {
            return I2mRegionModel::default();
        }
        let Some((left, right, top, bottom)) = editor.i2m_region_screen_bounds() else {
            return I2mRegionModel::default();
        };
        let content = lumino_ui_editor::grid::utils::content_bounds(editor);
        let min_x = left.max(content.x);
        let max_x = right.min(content.x + content.width);
        let min_y = top.max(content.y);
        let max_y = bottom.min(content.y + content.height);
        if max_x <= min_x || max_y <= min_y {
            return I2mRegionModel::default();
        }
        let canvas = &editor.editor_state.canvas;
        I2mRegionModel {
            rect: Some(egui::Rect::from_min_max(
                egui::pos2(min_x + canvas.offset_x, min_y + canvas.offset_y),
                egui::pos2(max_x + canvas.offset_x, max_y + canvas.offset_y),
            )),
        }
    }
}
