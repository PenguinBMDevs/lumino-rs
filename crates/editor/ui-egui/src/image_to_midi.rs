//! 图片转 MIDI 区域框（Placing 阶段）——egui 版。
//!
//! 与 旧版 `i2m_box::draw` 的框体一致：灰色 3px 边框 + 比边框浅的灰色半透明填充，
//! 按卷帘内容区裁剪（越界部分不显示，数据不动）。

use egui::{CornerRadius, Rect, Stroke, StrokeKind};

use crate::marquee::{FILL, STROKE, STROKE_WIDTH};

/// 区域框模型（窗口逻辑坐标，已裁剪）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct I2mRegionModel {
    /// 区域框；`None` = 不显示。
    pub rect: Option<Rect>,
}

/// 绘制区域框。
pub fn show(ui: &egui::Ui, model: &I2mRegionModel) {
    let Some(rect) = model.rect else {
        return;
    };
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::ZERO, FILL);
    painter.rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(STROKE_WIDTH, STROKE),
        StrokeKind::Middle,
    );
}
