//! 音频导出进度对话框（`DialogType::ExportProgress`）。
//!
//! 只读：标题 18px、状态行 14px（普通 / 错误红 / 完成绿）、30 高进度条
//! （`background.strong` 底 + `primary.base` 进度 + 圆角 2）、12px 百分比。
//! 整窗 base 底、内边距 24。
//!
//! 生产 `column.spacing(4)` 会叠加到每个 `space()` 上，这里用显式纵向偏移
//! （状态 46 / 进度条 84 / 百分比 122）得到同一套盒子位置。

use egui::{Align, CornerRadius, Rect, RichText, Vec2};

use crate::chrome::ExportProgressModel;
use crate::theme;

/// 整窗内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 标题行高（18px 文本）。
const TITLE_H: f32 = 22.0;
/// 状态行顶边 / 行高（14px 文本）。
const MESSAGE_TOP: f32 = 46.0;
const MESSAGE_H: f32 = 18.0;
/// 进度条顶边 / 高度（旧版 `ProgressBar::DEFAULT_GIRTH` = 30）。
const BAR_TOP: f32 = 84.0;
const BAR_H: f32 = 30.0;
/// 百分比顶边 / 行高（12px 文本）。
const PERCENT_TOP: f32 = 122.0;
const PERCENT_H: f32 = 16.0;

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &ExportProgressModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    label_block(
        ui,
        Rect::from_min_size(inner.min, Vec2::new(inner.width(), TITLE_H)),
        &model.title,
        18.0,
        theme::text(),
    );
    label_block(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left(), inner.top() + MESSAGE_TOP),
            Vec2::new(inner.width(), MESSAGE_H),
        ),
        &model.message,
        14.0,
        match model.kind {
            crate::chrome::ExportMessageKind::Normal => theme::text(),
            crate::chrome::ExportMessageKind::Error => theme::danger(),
            crate::chrome::ExportMessageKind::Completed => theme::success(),
        },
    );

    // 进度条：`background.strong` 底 + `primary.base` 进度，圆角 2（旧版 同款）。
    let bar = Rect::from_min_size(
        egui::pos2(inner.left(), inner.top() + BAR_TOP),
        Vec2::new(inner.width(), BAR_H),
    );
    let painter = ui.painter();
    painter.rect_filled(bar, CornerRadius::same(2), theme::selected_bg());
    let ratio = model.progress.clamp(0.0, 1.0);
    if ratio > 0.0 {
        painter.rect_filled(
            Rect::from_min_size(bar.min, Vec2::new(bar.width() * ratio, bar.height())),
            CornerRadius::same(2),
            theme::accent(),
        );
    }

    label_block(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left(), inner.top() + PERCENT_TOP),
            Vec2::new(inner.width(), PERCENT_H),
        ),
        &model.percentage,
        12.0,
        theme::text(),
    );
}

/// 固定矩形内的左对齐文本标签（保留可访问树节点）。
fn label_block(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32, color: egui::Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}

/// 百分比文本（生产 `format!("{:.1}%", progress * 100.0)`）。
pub fn percentage_text(progress: f64) -> String {
    format!("{:.1}%", progress.clamp(0.0, 1.0) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::percentage_text;

    #[test]
    fn percentage_uses_one_decimal() {
        assert_eq!(percentage_text(0.0), "0.0%");
        assert_eq!(percentage_text(0.331), "33.1%");
        assert_eq!(percentage_text(1.0), "100.0%");
        // 越界值夹紧（生产进度可能因浮点误差略超 1）。
        assert_eq!(percentage_text(1.2), "100.0%");
    }
}
