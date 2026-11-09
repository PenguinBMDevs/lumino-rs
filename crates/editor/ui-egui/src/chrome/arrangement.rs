//! 工程走带视图的 egui 绘制（第一步：顶部 28px 标尺）。
//!
//! 与 iced `ui-editor/src/arrangement/ruler.rs` 同口径：
//! - 底色 `ruler_background_color`、底部 1px 边框；
//! - 每个小节边界画一条 1px 竖线（贯穿整条标尺）；
//! - 小节号画在边界右侧 4px、顶部 4px；相邻标签间距小于 38px 时只画线不画字；
//! - 文本字号 `MEASURE_NUMBER_FONT_SIZE = 12.5`。
//!
//! 坐标约定：`tick_to_x = tick * zoom_x - scroll_x`（与 wgpu 网格同源）。

mod measures;

use egui::{Align2, FontId, Rect};

use crate::theme;

use super::ArrangementRulerModel;

/// 标尺高度（生产 `ARRANGEMENT_RULER_HEIGHT`）。
pub const RULER_HEIGHT: f32 = 28.0;
/// 相邻小节号标签的最小间距（像素）；更密时只画刻度线。
const MIN_LABEL_SPACING: f32 = 38.0;
/// 小节号字号（生产 `MEASURE_NUMBER_FONT_SIZE`）。
const LABEL_SIZE: f32 = 12.5;
/// 标签相对边界的内缩。
const LABEL_PAD: f32 = 4.0;

/// 计算某 tick 对应的屏幕 x（`rect` 左缘 = 走带区左缘）。
pub fn tick_to_x(rect: Rect, model: &ArrangementRulerModel, tick: f32) -> f32 {
    rect.left() + tick * model.zoom_x.max(1e-6) - model.scroll_x.max(0.0)
}

/// 绘制标尺（`rect` 由宿主按布局给出，宽度即走带区宽度）。
pub fn show_ruler(ui: &egui::Ui, rect: Rect, model: &ArrangementRulerModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 0, theme::roll_ruler_bg());
    // 底部 1px 边框（生产画在 `height - 0.5`，即最后一行像素）。
    painter.rect_filled(
        Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 1.0), rect.max),
        0,
        theme::border(),
    );

    let zoom = model.zoom_x.max(1e-6);
    let scroll = model.scroll_x.max(0.0);
    let start_tick = scroll / zoom;
    let end_tick = (scroll + rect.width()) / zoom;

    let mut last_label_x = f32::NEG_INFINITY;
    for (tick, number) in measures::measure_marks(
        &model.time_signatures,
        model.ppq as f32,
        start_tick,
        end_tick,
    ) {
        let x = rect.left() + tick * zoom - scroll;
        if x < rect.left() - 0.5 || x > rect.right() {
            continue;
        }
        painter.rect_filled(
            Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(1.0, rect.height())),
            0,
            theme::border(),
        );
        if x - last_label_x < MIN_LABEL_SPACING {
            continue;
        }
        let label_x = x + LABEL_PAD;
        if label_x + 12.0 > rect.right() {
            continue;
        }
        painter.text(
            egui::pos2(label_x, rect.top() + LABEL_PAD),
            Align2::LEFT_TOP,
            number.to_string(),
            FontId::proportional(LABEL_SIZE),
            theme::text(),
        );
        last_label_x = x;
    }
}
