//! 框选框（选择矩形）绘制——egui 版。
//!
//! 取代 旧版 `selection_box::draw`：统一「灰色 3px 边框 + 比边框浅的灰色半透明填充」。
//! 旧版 Canvas 的绘制层被 egui 底层纹理覆盖后不可见，本模块保证框选始终可见。

use egui::{Color32, CornerRadius, Rect, Stroke, StrokeKind};

/// 框选框边框色（0.62 灰）。
pub const STROKE: Color32 = Color32::from_rgb(158, 158, 158);
/// 填充色（0.78 灰，35% 透明）。
pub const FILL: Color32 = Color32::from_rgba_premultiplied(69, 69, 69, 89);
/// 边框宽度。
pub const STROKE_WIDTH: f32 = 3.0;

/// 框选框模型（窗口逻辑坐标，已裁剪到内容区）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarqueeModel {
    /// 全部矩形（拖拽中的选框 / 选中音符的包围盒；可能多个）。
    pub rects: Vec<Rect>,
}

impl MarqueeModel {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }
}

/// 绘制全部框选框。
pub fn show(ui: &egui::Ui, model: &MarqueeModel) {
    let painter = ui.painter();
    for rect in &model.rects {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        painter.rect_filled(*rect, CornerRadius::ZERO, FILL);
        painter.rect_stroke(
            *rect,
            CornerRadius::ZERO,
            Stroke::new(STROKE_WIDTH, STROKE),
            StrokeKind::Middle,
        );
    }
}
