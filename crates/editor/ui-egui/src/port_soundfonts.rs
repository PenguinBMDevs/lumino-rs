//! 端口音色库对话框（`DialogType::PortSoundfonts`，REND-005）。
//!
//! 版式：整窗 base 底、24px 内边距、标题 18px、说明 12px；
//! 每个端口一张卡片（端口标签 + 选择/清除按钮 + 路径行），
//! 底部 取消 / 确定。列表在 `ScrollArea` 内，端口多时滚动而不撑高窗口。
//!
//! 动作只回传「端口 + 意图」；文件选择、映射归一化与实时应用由生产侧负责。

use egui::{CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, Vec2};

use crate::chrome::{PortSoundfontsModel, PortSoundfontsRowModel};
use crate::theme;
use crate::widgets;

const ACTION_ID: &str = "lumino.port-soundfonts.action";
/// 整窗内边距。
const PAD: f32 = 24.0;
/// 区块间距。
const SECTION_GAP: f32 = 12.0;
/// 底部按钮尺寸。
const BUTTON_W: f32 = 100.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;

/// 端口音色库对话框产生的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSoundfontsAction {
    /// 为指定端口选择音色库文件。
    Browse(u8),
    /// 清除指定端口的覆盖。
    Clear(u8),
    /// 确定。
    Confirm,
    /// 取消。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: PortSoundfontsAction) {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).push_first(ctx, action);
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<PortSoundfontsAction> {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).take(ctx)
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &PortSoundfontsModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let body = crate::layout::body_rect(rect, model.titlebar);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 4.0;
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

    ScrollArea::vertical()
        .id_salt("lumino.port-soundfonts.scroll")
        .auto_shrink([false, false])
        .show(&mut child, |ui| form(ui, model, inner.width()));
}

fn form(ui: &mut egui::Ui, model: &PortSoundfontsModel, width: f32) {
    ui.add(egui::Label::new(
        RichText::new(&model.title).size(18.0).color(theme::text()),
    ));
    ui.add_space(6.0);
    ui.add(egui::Label::new(
        RichText::new(&model.hint)
            .size(12.0)
            .color(theme::text_dim()),
    ));
    ui.add_space(SECTION_GAP);

    for row in &model.rows {
        row_card(ui, row, width);
        ui.add_space(8.0);
    }

    ui.add_space(12.0);
    buttons(ui, model);
}

/// 单个端口卡片：标签 + 选择/清除 + 当前路径。
fn row_card(ui: &mut egui::Ui, row: &PortSoundfontsRowModel, width: f32) {
    let inner_width = (width - 20.0).max(1.0);
    Frame::NONE
        .fill(theme::card_bg())
        .stroke(Stroke::new(1.0, theme::border()))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(inner_width);
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(
                    RichText::new(&row.label).size(14.0).color(theme::text()),
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if small_button(ui, &row.clear, !row.path.is_empty()).clicked() {
                        push_action(ui.ctx(), PortSoundfontsAction::Clear(row.port));
                    }
                    if small_button(ui, &row.browse, true).clicked() {
                        push_action(ui.ctx(), PortSoundfontsAction::Browse(row.port));
                    }
                });
            });

            let (text, color) = if row.path.is_empty() {
                (row.path_hint.as_str(), theme::text_dim())
            } else {
                (row.path.as_str(), theme::text_muted())
            };
            ui.add(
                egui::Label::new(RichText::new(text).size(12.0).color(color))
                    .truncate()
                    .sense(egui::Sense::hover()),
            );
            if let Some(extra) = &row.extra {
                ui.add(egui::Label::new(
                    RichText::new(extra).size(11.0).color(theme::text_dim()),
                ));
            }
        });
}

/// 底部按钮：取消（次要）+ 12 + 确定（主色）。
fn buttons(ui: &mut egui::Ui, model: &PortSoundfontsModel) {
    ui.horizontal(|ui| {
        if button(ui, &model.cancel, theme::toolbar_bg(), theme::text()).clicked() {
            push_action(ui.ctx(), PortSoundfontsAction::Cancel);
        }
        ui.add_space(BUTTON_GAP);
        if button(ui, &model.ok, theme::accent(), theme::on_accent()).clicked() {
            push_action(ui.ctx(), PortSoundfontsAction::Confirm);
        }
    });
}

fn small_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let button = widgets::text_button(RichText::new(label).size(12.0).color(theme::text()))
        .fill(theme::toolbar_bg())
        .corner_radius(CornerRadius::same(4));
    ui.add_enabled(enabled, button)
}

fn button(
    ui: &mut egui::Ui,
    label: &str,
    fill: egui::Color32,
    text: egui::Color32,
) -> egui::Response {
    let button = widgets::text_button(RichText::new(label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.add_sized(Vec2::new(BUTTON_W, BUTTON_H), button)
}
