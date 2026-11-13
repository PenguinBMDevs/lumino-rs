//! 「音符画设置」对话框（`DialogType::DrawSettings`）。
//!
//! 结构同 `brush_settings_dialog`：内边距 16、标题 18px、说明 13px（自动换行）、
//! 间距 12、底部右对齐「关闭」。当前内容仍是占位说明（生产同文案），
//! 不摆点了没反应的假控件。

use egui::{Align, CornerRadius, FontId, RichText, Vec2};

use crate::chrome::DrawSettingsModel;
use crate::theme;

const PAD: f32 = 16.0;
/// 底部按钮（生产 `padding([6,16])` + 14px 文本）。
const BUTTON_H: f32 = 30.0;

/// 对话框产生的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawSettingsAction {
    /// 关闭窗口。
    Close,
}

/// 记录本帧动作。
pub fn push_action(ctx: &egui::Context, action: DrawSettingsAction) {
    let key = egui::Id::unique("lumino.draw-settings.action");
    if ctx
        .data(|d| d.get_temp::<Option<DrawSettingsAction>>(key))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(key, Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<DrawSettingsAction> {
    ctx.data_mut(|d| {
        d.remove_temp::<Option<DrawSettingsAction>>(egui::Id::unique("lumino.draw-settings.action"))
    })
    .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &DrawSettingsModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(Align::Min)),
    );
    child.spacing_mut().item_spacing = Vec2::ZERO;
    child.add(egui::Label::new(
        RichText::new(&model.title).size(18.0).color(theme::text()),
    ));
    child.add_space(12.0);
    child.add(
        egui::Label::new(
            RichText::new(&model.placeholder)
                .size(theme::font::BODY)
                .color(theme::text()),
        )
        .wrap(),
    );

    // 底部右对齐「关闭」：按文本宽度 + 32（生产 padding [6,16]）。
    let close_w = ui
        .painter()
        .layout_no_wrap(
            model.close.clone(),
            FontId::proportional(theme::font::LABEL),
            theme::text(),
        )
        .size()
        .x
        .ceil()
        + 32.0;
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(inner.right() - close_w, inner.bottom() - BUTTON_H),
        Vec2::new(close_w, BUTTON_H),
    );
    let close = egui::Button::new(
        RichText::new(&model.close)
            .size(theme::font::LABEL)
            .color(theme::text()),
    )
    .fill(theme::toolbar_bg())
    .corner_radius(CornerRadius::same(4));
    if ui.put(close_rect, close).clicked() {
        push_action(ui.ctx(), DrawSettingsAction::Close);
    }
}
