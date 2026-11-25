//! 「端口音色库未生效」提示窗（REND-005）。
//!
//! 当前后端不支持按端口音色库时展示：映射已保存，但播放/导出降级使用
//! 全局音色库。用户可继续降级，或打开合成器设置切到 XSynth。

use egui::{CornerRadius, RichText, Vec2};

use crate::chrome::PortSoundfontsUnsupportedModel;
use crate::theme;
use crate::widgets;

const ACTION_ID: &str = "lumino.port-soundfonts-unsupported.action";
const PAD: f32 = 24.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;

/// 提示窗产生的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSoundfontsUnsupportedAction {
    /// 继续使用全局音色库（降级运行），关闭提示。
    Acknowledge,
    /// 打开合成器设置（用户可切换到 XSynth）。
    OpenSettings,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: PortSoundfontsUnsupportedAction) {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).push_first(ctx, action);
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<PortSoundfontsUnsupportedAction> {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).take(ctx)
}

/// 渲染整个提示窗（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &PortSoundfontsUnsupportedModel) {
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
    child.spacing_mut().item_spacing.y = 8.0;

    child.add(egui::Label::new(
        RichText::new(&model.title).size(18.0).color(theme::text()),
    ));
    child.add(egui::Label::new(
        RichText::new(&model.message)
            .size(14.0)
            .color(theme::warning()),
    ));
    child.add(egui::Label::new(
        RichText::new(&model.detail)
            .size(12.0)
            .color(theme::text_muted()),
    ));

    child.add_space(16.0);
    let width = inner.width();
    child.horizontal(|ui| {
        let ack_w = (width - BUTTON_GAP) * 0.62;
        let settings_w = (width - BUTTON_GAP - ack_w).max(1.0);
        if button(
            ui,
            &model.acknowledge,
            Vec2::new(ack_w, BUTTON_H),
            theme::accent(),
            theme::on_accent(),
        )
        .clicked()
        {
            push_action(ui.ctx(), PortSoundfontsUnsupportedAction::Acknowledge);
        }
        ui.add_space(BUTTON_GAP);
        if button(
            ui,
            &model.open_settings,
            Vec2::new(settings_w, BUTTON_H),
            theme::toolbar_bg(),
            theme::text(),
        )
        .clicked()
        {
            push_action(ui.ctx(), PortSoundfontsUnsupportedAction::OpenSettings);
        }
    });
}

fn button(
    ui: &mut egui::Ui,
    label: &str,
    size: Vec2,
    fill: egui::Color32,
    text: egui::Color32,
) -> egui::Response {
    let button = widgets::text_button(RichText::new(label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.add_sized(size, button)
}
