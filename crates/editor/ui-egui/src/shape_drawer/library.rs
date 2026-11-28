//! 形状抽屉的库列表：可滚动、行距 2、整行主色底。

use crate::chrome::{ShapeDrawerModel, ShapePanelAction};
use crate::theme;
use egui::{Align, CornerRadius, Vec2};

use super::layout::{ROW_GAP, ROW_H};
use super::widgets::{act, label_left_in};

/// 库列表（可滚动，行距 2）。
pub(super) fn library(ui: &mut egui::Ui, rect: egui::Rect, model: &ShapeDrawerModel) {
    let ctx = ui.ctx().clone();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
    );
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 8.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = false;
    }
    let visuals = child.visuals_mut();
    visuals.widgets.inactive.fg_stroke.color = theme::selected_bg();
    visuals.widgets.hovered.fg_stroke.color = theme::hover_border();
    visuals.widgets.active.fg_stroke.color = theme::active_bg();

    egui::ScrollArea::vertical()
        .id_salt("lumino.shape-drawer.library")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            let width = ui.available_width().max(1.0);
            for row in &model.rows {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(width, ROW_H), egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, true, row.selected, &row.label)
                });
                // 上一代 默认 `button`：库行整行主色底 + 白字（选中态靠「● 」前缀）。
                let fill = if response.hovered() {
                    theme::accent_strong()
                } else {
                    theme::accent()
                };
                ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
                label_left_in(
                    ui,
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 4.0, rect.top()),
                        egui::pos2(rect.right(), rect.bottom()),
                    ),
                    &row.label,
                    16.0,
                    theme::on_accent(),
                );
                if response.clicked() {
                    act(&ctx, ShapePanelAction::Select(row.id.clone()));
                }
            }
        });
}
