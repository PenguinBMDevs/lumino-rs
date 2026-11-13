//! 文字参数面板的字体下拉（输入框 + 弹层）。
//!
//! 从 `text_panel.rs` 拆出（REF-001 行数纪律）。弹层由调用方在面板内容之后
//! 绘制，保证盖住滑杆行；其矩形同时进入占用列表，点击不会穿透到 旧版。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::super::TextParamsPanelModel;
use super::super::panels::{self, TEXT, TEXT_DIM};
use super::push_changed;
use crate::theme;

const FONT_OPEN_ID: &str = "lumino.text-panel.font-open";

/// 字体下拉输入框（只画框 + 开合状态；列表由 [`font_popup`] 最后绘制）。
pub(super) fn field(ui: &mut egui::Ui, rect: Rect, model: &TextParamsPanelModel) -> bool {
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        Pos2::new(rect.left() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        &model.params.font,
        FontId::proportional(12.0),
        TEXT,
    );
    // 下拉三角用矢量（egui 默认字体没有 ▾，会画成方框）。
    let cx = rect.right() - 9.0;
    let cy = rect.center().y;
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(cx - 4.0, cy - 2.0),
            Pos2::new(cx + 4.0, cy - 2.0),
            Pos2::new(cx, cy + 3.0),
        ],
        TEXT_DIM,
        Stroke::NONE,
    ));
    let response = ui.interact(
        rect,
        egui::Id::unique("lumino.text-panel.font-field"),
        Sense::click(),
    );
    let mut open: bool = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::unique(FONT_OPEN_ID)))
        .unwrap_or(false);
    if response.clicked() {
        open = !open;
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(FONT_OPEN_ID), open));
    open
}

/// 字体列表弹层（在面板内容之后绘制，盖住滑杆行而不是被它们盖住）。
pub(super) fn popup(
    ui: &mut egui::Ui,
    field: Rect,
    model: &TextParamsPanelModel,
    blocked: &mut Vec<Rect>,
) {
    let rows = model.fonts.len().max(1) as f32;
    let height = (rows * 18.0 + 8.0).min(220.0);
    let mut origin = Pos2::new(field.left(), field.bottom() + 2.0);
    let viewport = ui.ctx().viewport_rect();
    if origin.y + height > viewport.bottom() {
        origin.y = field.top() - height - 2.0;
    }
    origin.y = origin.y.max(viewport.top());
    let popup = Rect::from_min_size(origin, Vec2::new(field.width().max(150.0), height));
    ui.painter().rect(
        popup,
        CornerRadius::same(6),
        panels::panel_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(popup.shrink(4.0))
            .layout(egui::Layout::top_down(Align::Min)),
    );
    let mut open = true;
    egui::ScrollArea::vertical()
        .id_salt("lumino.text-panel.fonts")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            for font in &model.fonts {
                let selected = font.eq_ignore_ascii_case(&model.params.font);
                let button = egui::Button::new(egui::RichText::new(font).size(12.0).color(TEXT))
                    .frame(false)
                    .fill(if selected {
                        panels::selection_fill()
                    } else {
                        Color32::TRANSPARENT
                    })
                    .min_size(Vec2::new(ui.available_width(), 18.0));
                if ui.add(button).clicked() {
                    let mut next = model.params.clone();
                    next.font = font.clone();
                    push_changed(ui, next);
                    open = false;
                }
            }
        });
    blocked.push(popup);

    // 点击列表与输入框以外关闭列表（面板本身的外部关闭逻辑另行处理）。
    if open
        && ui.input(|i| i.pointer.any_click())
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|pos| !popup.contains(pos) && !field.contains(pos))
    {
        open = false;
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(FONT_OPEN_ID), open));
}
