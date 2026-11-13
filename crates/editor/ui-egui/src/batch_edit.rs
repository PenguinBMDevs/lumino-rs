//! 批量编辑对话框（`DialogType::BatchEdit`）。
//!
//! 几何对齐生产 `view/batch_edit_dialog.rs`：整窗 base 底、内边距 24、标题 18px；
//! 两张圆角 16 的卡片（1px `strong` 边框、内边距 16）：「设置」四行
//! （标签 110 + 8 + 输入 120×28，间距 12）与「格式」说明；底部右对齐
//! 「确定」（主色）/「取消」（次要）。
//!
//! 输入只回传原样文本，表达式解析与运算是生产侧职责。

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, RichText, Stroke, Vec2,
};

use crate::chrome::{BatchEditFieldModel, BatchEditModel};
use crate::theme;

const ACTION_ID: &str = "lumino.batch-edit.action";
const PAD: f32 = 24.0;
const LABEL_W: f32 = 110.0;
const INPUT_W: f32 = 120.0;
const INPUT_H: f32 = 28.0;
const BTN_W: f32 = 76.0;
const BTN_H: f32 = 30.0;

/// 批量编辑对话框产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchEditAction {
    /// 某个字段的文本变化（原样回传）。
    InputChanged(BatchEditFieldModel, String),
    /// 确定：生产侧解析并应用。
    Confirm,
    /// 取消：关闭对话框。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: BatchEditAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<BatchEditAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<BatchEditAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<BatchEditAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &BatchEditModel) {
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

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    child.spacing_mut().item_spacing = Vec2::ZERO;

    child.add(egui::Label::new(
        RichText::new(&model.title).size(18.0).color(theme::text()),
    ));
    child.add_space(24.0);

    card(&mut child, |ui| {
        ui.add(egui::Label::new(
            RichText::new(&model.settings_title)
                .size(16.0)
                .color(theme::text()),
        ));
        ui.add_space(32.0);
        for (index, row) in model.rows.iter().enumerate() {
            if index > 0 {
                ui.add_space(12.0);
            }
            input_row(ui, row);
        }
    });

    child.add_space(20.0);
    card(&mut child, |ui| {
        ui.add(egui::Label::new(
            RichText::new(&model.format_title)
                .size(16.0)
                .color(theme::text()),
        ));
        ui.add_space(20.0);
        for (index, hint) in model.format_hints.iter().enumerate() {
            if index > 0 {
                ui.add_space(6.0);
            }
            let size = if index == 0 { 13.0 } else { 12.0 };
            ui.add(egui::Label::new(
                RichText::new(hint).size(size).color(theme::text_muted()),
            ));
        }
    });

    child.add_space(28.0);
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if button(ui, &model.cancel, theme::toolbar_bg(), theme::text()).clicked() {
            push_action(ui.ctx(), BatchEditAction::Cancel);
        }
        ui.add_space(12.0);
        if button(ui, &model.ok, theme::accent(), theme::on_accent()).clicked() {
            push_action(ui.ctx(), BatchEditAction::Confirm);
        }
    });
}

/// 圆角 16 的卡片（base 底 + 1px `strong` 边框 + 16 内边距）。
fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    Frame::NONE
        .fill(theme::base_bg())
        .stroke(Stroke::new(1.0, theme::selected_bg()))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            add(ui);
        });
}

/// 一行：固定 110 宽标签 + 8px + 120×28 输入框。
fn input_row(ui: &mut egui::Ui, row: &crate::chrome::BatchEditRowModel) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // 固定 110 宽标签列：先占用整列宽度，再把标签左上对齐放进该矩形
        // （`allocate_ui_with_layout` 只按内容宽度前进，会把输入框提前）。
        let (label_rect, _) =
            ui.allocate_exact_size(Vec2::new(LABEL_W, INPUT_H), egui::Sense::hover());
        let mut label_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(label_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        label_ui.add(egui::Label::new(
            RichText::new(&row.label).size(14.0).color(theme::text()),
        ));
        ui.add_space(8.0);
        let mut buffer = row.value.clone();
        let response = input_frame().show(ui, |ui| {
            ui.set_min_width(INPUT_W - 20.0);
            ui.set_min_height(INPUT_H - 12.0);
            ui.push_id(("batch-edit", row.field as u8), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut buffer)
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(FontId::proportional(14.0))
                        .text_color(theme::text())
                        .hint_text(
                            RichText::new(&row.placeholder)
                                .size(14.0)
                                .color(theme::text_dim()),
                        )
                        .desired_width(INPUT_W - 20.0),
                )
            })
            .inner
        });
        if response.inner.changed() {
            push_action(ui.ctx(), BatchEditAction::InputChanged(row.field, buffer));
        }
    });
}

/// 输入框外框：`background.weak` 底 + 1px `strong` 边框、圆角 4。
fn input_frame() -> Frame {
    Frame::NONE
        .fill(theme::base_bg())
        .stroke(Stroke::new(1.0, theme::selected_bg()))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 6))
}

fn button(ui: &mut egui::Ui, label: &str, fill: Color32, text: Color32) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.add_sized(Vec2::new(BTN_W, BTN_H), button)
}
