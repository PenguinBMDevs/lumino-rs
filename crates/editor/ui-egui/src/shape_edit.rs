//! 图形选中框 / 编辑框 / 8+1 锚点（DRAW-020 · DRAW-041C）——egui 版。
//!
//! 取代旧版 `drawn_shape_box::draw` 中与选中 / 编辑相关的可见部分：
//! - 选中选框：主题色 1.5px 描边 + 四角实心手柄；
//! - 编辑框：进入「编辑图形」后替代选中选框的 1px 细框；
//! - 箱体锚点：8 个 8px 白底方块 + 主题色描边；
//! - 中心锚点：白底圆 + 十字（示意「整体平移」）。
//!
//! 全部坐标为窗口逻辑坐标（生产侧已加画布偏移）。

use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind};

use crate::theme;

/// 选中选框描边宽度。
const BOX_STROKE_WIDTH: f32 = 1.5;
/// 选中选框四角手柄边长。
const BOX_HANDLE_SIZE: f32 = 6.0;
/// 选中选框描边透明度（0.85）。
const BOX_STROKE_ALPHA: u8 = 217;
/// 编辑框描边透明度（0.65）。
const EDIT_BOX_ALPHA: u8 = 166;
/// 编辑态锚点边长。
const EDIT_HANDLE_SIZE: f32 = 8.0;
/// 中心锚点半径。
const CENTER_RADIUS: f32 = 5.0;

/// 本帧的选中框 / 编辑框 / 锚点 / 图形轮廓。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapeEditModel {
    /// 常显轮廓（鼠标工具下当前轨未选中的可见图形）。
    pub idle_outlines: Vec<Vec<Pos2>>,
    /// 选中图形轮廓（逐条折线）。
    pub selected_outlines: Vec<Vec<Pos2>>,
    /// 框选矩形（图形工具拉框时）。
    pub marquee: Option<Rect>,
    /// 选中选框（编辑态下隐藏）。
    pub selection_box: Option<Rect>,
    /// 编辑框（仅编辑态）。
    pub edit_box: Option<Rect>,
    /// 8 个箱体锚点。
    pub handles: Vec<Pos2>,
    /// 中心锚点（仅未 Bake 组合）。
    pub center: Option<Pos2>,
}

impl ShapeEditModel {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.idle_outlines.is_empty()
            && self.selected_outlines.is_empty()
            && self.marquee.is_none()
            && self.selection_box.is_none()
            && self.edit_box.is_none()
            && self.handles.is_empty()
            && self.center.is_none()
    }
}

/// 给颜色套一个透明度（保留 RGB）。
fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// 绘制选中框 / 编辑框 / 锚点。
pub fn show(ui: &egui::Ui, model: &ShapeEditModel) {
    let painter = ui.painter();
    let accent = theme::accent_strong();

    // 常显轮廓：弱化细线，避免盖过音符。
    for outline in &model.idle_outlines {
        if outline.len() >= 2 {
            painter.add(egui::Shape::line(
                outline.clone(),
                Stroke::new(1.5, with_alpha(accent, 140)),
            ));
        }
    }
    // 选中高亮：外发光 + 实线双描边（与旧版同口径）。
    for outline in &model.selected_outlines {
        if outline.len() < 2 {
            continue;
        }
        painter.add(egui::Shape::line(
            outline.clone(),
            Stroke::new(7.0, with_alpha(accent, 76)),
        ));
        painter.add(egui::Shape::line(outline.clone(), Stroke::new(2.5, accent)));
    }

    // 框选矩形：主题色 12% 填充 + 75% 描边（与旧版同口径）。
    if let Some(rect) = model.marquee {
        painter.rect_filled(rect, CornerRadius::ZERO, with_alpha(accent, 31));
        painter.rect_stroke(
            rect,
            CornerRadius::ZERO,
            Stroke::new(1.0, with_alpha(accent, 191)),
            StrokeKind::Middle,
        );
    }

    if let Some(rect) = model.selection_box {
        painter.rect_stroke(
            rect,
            CornerRadius::ZERO,
            Stroke::new(BOX_STROKE_WIDTH, with_alpha(accent, BOX_STROKE_ALPHA)),
            StrokeKind::Middle,
        );
        let half = BOX_HANDLE_SIZE / 2.0;
        for corner in [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ] {
            let handle = Rect::from_min_size(
                egui::pos2(corner.x - half, corner.y - half),
                egui::vec2(BOX_HANDLE_SIZE, BOX_HANDLE_SIZE),
            );
            painter.rect_filled(handle, CornerRadius::ZERO, accent);
        }
    }

    if let Some(rect) = model.edit_box {
        painter.rect_stroke(
            rect,
            CornerRadius::ZERO,
            Stroke::new(1.0, with_alpha(accent, EDIT_BOX_ALPHA)),
            StrokeKind::Middle,
        );
    }

    let half = EDIT_HANDLE_SIZE / 2.0;
    for point in &model.handles {
        let handle = Rect::from_min_size(
            egui::pos2(point.x - half, point.y - half),
            egui::vec2(EDIT_HANDLE_SIZE, EDIT_HANDLE_SIZE),
        );
        painter.rect_filled(handle, CornerRadius::ZERO, Color32::WHITE);
        painter.rect_stroke(
            handle,
            CornerRadius::ZERO,
            Stroke::new(1.5, accent),
            StrokeKind::Inside,
        );
    }

    if let Some(center) = model.center {
        painter.circle_filled(center, CENTER_RADIUS, Color32::WHITE);
        painter.circle_stroke(center, CENTER_RADIUS, Stroke::new(1.5, accent));
        let arm = CENTER_RADIUS - 2.0;
        painter.line_segment(
            [
                egui::pos2(center.x - arm, center.y),
                egui::pos2(center.x + arm, center.y),
            ],
            Stroke::new(1.2, accent),
        );
        painter.line_segment(
            [
                egui::pos2(center.x, center.y - arm),
                egui::pos2(center.x, center.y + arm),
            ],
            Stroke::new(1.2, accent),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model_is_empty() {
        assert!(ShapeEditModel::default().is_empty());
    }

    #[test]
    fn test_any_field_makes_model_non_empty() {
        let model = ShapeEditModel {
            handles: vec![egui::pos2(1.0, 2.0)],
            ..ShapeEditModel::default()
        };
        assert!(!model.is_empty());

        let model = ShapeEditModel {
            center: Some(egui::pos2(1.0, 2.0)),
            ..ShapeEditModel::default()
        };
        assert!(!model.is_empty());

        let model = ShapeEditModel {
            selection_box: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4.0, 4.0))),
            ..ShapeEditModel::default()
        };
        assert!(!model.is_empty());
    }
}
