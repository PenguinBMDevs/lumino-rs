//! 区域着色调色板（DRAW-032B）——egui 版。
//!
//! 取代旧版 `area_paint_box::draw` 的可见部分：
//! - 圆角面板 + 1px 边框；
//! - 标题「区域着色」（未启用通道 10 时追加提示）；
//! - 17 个色块（0 = 清除，画斜杠；16 = 通道 10，未启用时置灰）；
//! - 当前色号白描边 + 右上角 ×。
//!
//! 点击命中仍在 `lumino_ui_editor::grid::canvas_input`，本模块只负责绘制。

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke, StrokeKind};

use crate::theme;

/// 面板圆角。
const PANEL_RADIUS: f32 = 6.0;
/// 色块圆角。
const SWATCH_RADIUS: f32 = 3.0;
/// 标题字号。
const TITLE_SIZE: f32 = 12.0;
/// 关闭按钮字号。
const CLOSE_SIZE: f32 = 14.0;

/// 一个色块。
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteSwatch {
    /// 色块矩形（窗口逻辑坐标）。
    pub rect: Rect,
    /// 展示色。
    pub color: Color32,
    /// 是否为当前色号。
    pub active: bool,
    /// 是否不可用（16 号色且未启用通道 10）。
    pub disabled: bool,
    /// 是否为「清除」槽（0 号色，画斜杠）。
    pub clear: bool,
}

/// 调色板模型。
#[derive(Debug, Clone, PartialEq)]
pub struct AreaPaletteModel {
    /// 面板矩形。
    pub panel: Rect,
    /// 标题。
    pub title: String,
    /// 色块。
    pub swatches: Vec<PaletteSwatch>,
    /// 关闭按钮矩形。
    pub close: Rect,
}

impl Default for AreaPaletteModel {
    fn default() -> Self {
        let zero = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ZERO);
        Self {
            panel: zero,
            title: String::new(),
            swatches: Vec::new(),
            close: zero,
        }
    }
}

impl AreaPaletteModel {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.swatches.is_empty()
    }
}

/// 绘制调色板。
pub fn show(ui: &egui::Ui, model: &AreaPaletteModel) {
    if model.is_empty() {
        return;
    }
    let painter = ui.painter();
    let radius = CornerRadius::from(PANEL_RADIUS);
    painter.rect_filled(model.panel, radius, theme::panel_bg());
    painter.rect_stroke(
        model.panel,
        radius,
        Stroke::new(1.0, theme::border()),
        StrokeKind::Inside,
    );

    painter.text(
        egui::pos2(model.panel.left() + 8.0, model.panel.top() + 5.0),
        Align2::LEFT_TOP,
        &model.title,
        FontId::proportional(TITLE_SIZE),
        theme::text(),
    );
    painter.text(
        egui::pos2(model.close.center().x, model.close.top() - 1.0),
        Align2::CENTER_TOP,
        "×",
        FontId::proportional(CLOSE_SIZE),
        theme::text(),
    );

    let swatch_radius = CornerRadius::from(SWATCH_RADIUS);
    for swatch in &model.swatches {
        painter.rect_filled(swatch.rect, swatch_radius, swatch.color);
        if swatch.active {
            painter.rect_stroke(
                swatch.rect,
                swatch_radius,
                Stroke::new(2.0, Color32::WHITE),
                StrokeKind::Inside,
            );
        }
        if swatch.disabled {
            painter.rect_filled(swatch.rect, swatch_radius, Color32::from_black_alpha(128));
        }
        if swatch.clear {
            // 0 = 清除：画一道斜杠，避免和「深灰」混淆。
            painter.line_segment(
                [
                    egui::pos2(swatch.rect.left() + 4.0, swatch.rect.bottom() - 4.0),
                    egui::pos2(swatch.rect.right() - 4.0, swatch.rect.top() + 4.0),
                ],
                Stroke::new(2.0, Color32::WHITE),
            );
        }
    }
}

/// 面板整体矩形（供输入占用 / 测试）。
pub fn panel_rect(model: &AreaPaletteModel) -> Option<Rect> {
    (!model.is_empty()).then_some(model.panel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swatch(rect: Rect, clear: bool, disabled: bool) -> PaletteSwatch {
        PaletteSwatch {
            rect,
            color: Color32::WHITE,
            active: false,
            disabled,
            clear,
        }
    }

    #[test]
    fn test_model_without_swatches_is_empty() {
        assert!(AreaPaletteModel::default().is_empty());
    }

    #[test]
    fn test_model_keeps_slot_flags() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4.0, 4.0));
        let model = AreaPaletteModel {
            panel: rect,
            title: "区域着色".to_string(),
            swatches: vec![swatch(rect, true, false), swatch(rect, false, true)],
            close: rect,
        };
        assert!(!model.is_empty());
        assert!(model.swatches[0].clear);
        assert!(model.swatches[1].disabled);
        assert_eq!(panel_rect(&model), Some(rect));
    }
}
