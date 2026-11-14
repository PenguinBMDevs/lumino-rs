//! 形状列表（只读，DRAW-041B）——egui 版。
//!
//! 取代旧版 `shape_list_box::draw` 的可见部分：
//! - 圆角面板 + 1px 边框；
//! - 标题「形状列表（只读）」+ 右上角 ×；
//! - 隔行底色 + 模型名（id 兜底由生产侧生成）。
//!
//! 点击命中仍在 `lumino_ui_editor::grid::canvas_input`，本模块只负责绘制。

use egui::{Align2, CornerRadius, FontId, Rect, Stroke, StrokeKind};

use crate::theme;

/// 面板圆角。
const PANEL_RADIUS: f32 = 6.0;
/// 标题字号。
const TITLE_SIZE: f32 = 12.0;
/// 行字号。
const ROW_SIZE: f32 = 12.0;
/// 关闭按钮字号。
const CLOSE_SIZE: f32 = 14.0;

/// 一行模型。
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeListRow {
    /// 行矩形（窗口逻辑坐标）。
    pub rect: Rect,
    /// 展示名（空名时由生产侧拼「形状 #id」）。
    pub label: String,
}

/// 形状列表面板。
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeListModel {
    /// 面板矩形。
    pub panel: Rect,
    /// 标题。
    pub title: String,
    /// 模型行。
    pub rows: Vec<ShapeListRow>,
    /// 关闭按钮矩形。
    pub close: Rect,
}

impl Default for ShapeListModel {
    fn default() -> Self {
        let zero = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ZERO);
        Self {
            panel: zero,
            title: String::new(),
            rows: Vec::new(),
            close: zero,
        }
    }
}

impl ShapeListModel {
    /// 是否没有内容（没有模型行 = 不显示）。
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// 绘制形状列表。
pub fn show(ui: &egui::Ui, model: &ShapeListModel) {
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
        egui::pos2(model.panel.left() + 8.0, model.panel.top() + 7.0),
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

    for (index, row) in model.rows.iter().enumerate() {
        if index % 2 == 0 {
            painter.rect_filled(row.rect, CornerRadius::ZERO, theme::card_bg());
        }
        // 行文本按行矩形裁剪，避免长模型名压出面板。
        let row_painter = painter.with_clip_rect(row.rect);
        row_painter.text(
            egui::pos2(row.rect.left() + 6.0, row.rect.top() + 4.0),
            Align2::LEFT_TOP,
            &row.label,
            FontId::proportional(ROW_SIZE),
            theme::text(),
        );
    }
}

/// 面板整体矩形（供输入占用 / 测试）。
pub fn panel_rect(model: &ShapeListModel) -> Option<Rect> {
    (!model.is_empty()).then_some(model.panel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_without_rows_is_empty() {
        let model = ShapeListModel {
            panel: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(10.0, 10.0)),
            title: "形状列表（只读）".to_string(),
            rows: Vec::new(),
            close: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4.0, 4.0)),
        };
        assert!(model.is_empty());
        assert_eq!(panel_rect(&model), None);
    }

    #[test]
    fn test_model_with_row_is_visible() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(10.0, 10.0));
        let model = ShapeListModel {
            panel: rect,
            title: "形状列表（只读）".to_string(),
            rows: vec![ShapeListRow {
                rect,
                label: "形状 #1".to_string(),
            }],
            close: rect,
        };
        assert!(!model.is_empty());
        assert_eq!(panel_rect(&model), Some(rect));
    }
}
