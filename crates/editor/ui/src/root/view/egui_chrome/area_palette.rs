//! 区域着色调色板：生产状态 → egui 模型。
//!
//! 几何来自 `area_paint_box::palette_rects`（与命中测试同源），颜色来自
//! `area_paint_box::slot_color`，这里只做坐标换算与色值转换。

use egui::Color32;
use lumino_core::tr;
use lumino_ui_editor::grid::area_paint_box::{palette_rects, slot_color};
use lumino_ui_egui::area_palette::{AreaPaletteModel, PaletteSwatch};

use crate::root::Root;

impl Root {
    /// 本帧区域着色调色板（空模型 = 不显示）。
    pub fn area_palette_model(&self) -> AreaPaletteModel {
        let editor = &self.editor;
        let Some(rects) = palette_rects(editor) else {
            return AreaPaletteModel::default();
        };
        let canvas = &editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let to_rect = |rect: lumino_ui_core::geom::Rectangle| {
            egui::Rect::from_min_size(
                egui::pos2(rect.x + offset.x, rect.y + offset.y),
                egui::vec2(rect.width, rect.height),
            )
        };
        let to_u8 = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;

        let swatches = rects
            .swatches
            .iter()
            .enumerate()
            .map(|(slot, rect)| {
                let color = slot_color(slot as u8);
                PaletteSwatch {
                    rect: to_rect(*rect),
                    color: Color32::from_rgb(to_u8(color.r), to_u8(color.g), to_u8(color.b)),
                    active: usize::from(rects.active) == slot,
                    disabled: slot == 16 && !rects.use10,
                    clear: slot == 0,
                }
            })
            .collect();

        let title = if rects.use10 {
            tr!("区域着色").to_string()
        } else {
            tr!("区域着色 · 16 需启用通道 10").to_string()
        };

        AreaPaletteModel {
            panel: to_rect(rects.panel),
            title,
            swatches,
            close: to_rect(rects.close),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::storage::config::UiConfig;

    #[test]
    fn test_inactive_palette_has_empty_model() {
        let root = Root::new(&UiConfig::default());
        assert!(root.area_palette_model().is_empty());
    }
}
