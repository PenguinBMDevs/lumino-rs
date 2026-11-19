//! 混音台绘制与交互（对话框内容区）。
//!
//! 通道条卡片 / 电平表 / 底部按钮仍是 painter 几何绘制；音量推子改为标准
//! egui 垂直 [`egui::Slider`]，由 egui 负责绘制与输入。交互结果统一写入
//! [`super::MixerAction`] 通道，由 Runner 在对话框重绘后取走并回放到主窗口。

use egui::{Align2, Color32, CornerRadius, FontId, NumExt, Rect, Stroke};

use super::{
    METER_WIDTH, MixerAction, MixerDialogModel, MixerHit, MixerLayout, MixerModel, MixerStrip,
    hit_test, push_action,
};
use crate::{layout, theme};

/// 绘制整个混音台对话框，并把本帧交互写入动作通道。
pub fn show_dialog(ui: &mut egui::Ui, model: &MixerDialogModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let layout = MixerLayout::dialog(ui.max_rect(), model.titlebar, &model.mixer);
    if layout.panel.width() <= 0.0
        || layout.panel.height() <= 0.0
        || layout.body.width() <= 0.0
        || layout.body.height() <= 0.0
    {
        return;
    }
    // 自制标题栏先于本模块绘制：底色只能铺标题栏**以下**，否则会把红绿灯盖掉。
    let content = Rect::from_min_max(
        egui::pos2(
            layout.panel.left(),
            layout.panel.top()
                + if model.titlebar {
                    layout::TITLEBAR_H
                } else {
                    0.0
                },
        ),
        layout.panel.max,
    );
    ui.painter().rect_filled(content, 0, theme::base_bg());

    let mut actions = Vec::new();

    // 普通通道条：卡片 / 电平 / 按钮用 painter 画，推子用标准滑杆。
    // 整组裁剪到视口，滚动到边缘的通道条不会压到右侧主控条。
    {
        let mut strips_ui = ui.new_child(egui::UiBuilder::new().max_rect(layout.strips_viewport));
        strips_ui.shrink_clip_rect(layout.strips_viewport);
        for index in layout.visible(model.mixer.strips.len(), model.mixer.scroll_x) {
            let Some(strip) = model.mixer.strips.get(index) else {
                continue;
            };
            let rect = layout.strip_rect(index, model.mixer.scroll_x);
            draw_strip(strips_ui.painter(), rect, strip, true);
            if let Some(value) = show_fader(
                &mut strips_ui,
                rect,
                strip.volume,
                true,
                ("mixer-fader", index),
            ) {
                actions.push(MixerAction::Gain { index, value });
            }
        }
    }

    if let (Some(rect), Some(master)) = (layout.master, model.mixer.master.as_ref()) {
        draw_strip(ui.painter(), rect, master, false);
        if let Some(value) = show_fader(ui, rect, master.volume, false, ("mixer-fader", "master")) {
            actions.push(MixerAction::MasterVolume(value));
        }
    }

    if model.mixer.strips.is_empty() && model.mixer.master.is_none() {
        ui.painter().text(
            layout.body.center(),
            Align2::CENTER_CENTER,
            "暂无音轨",
            FontId::proportional(12.0),
            theme::text_muted(),
        );
    }

    actions.extend(interact(ui, &layout, &model.mixer));
    for action in actions {
        push_action(ui.ctx(), action);
    }
}

/// 标准 egui 垂直滑杆（推子）；返回本帧变更后的音量。
fn show_fader(
    ui: &mut egui::Ui,
    strip: Rect,
    volume: f32,
    with_buttons: bool,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
) -> Option<f32> {
    let rect = super::fader_rect(strip, with_buttons);
    // 竖向滑杆的自身矩形 = (thickness, slider_width)。把矩形收窄到 thickness
    // 并居中，轨道才落在通道条中线上；`put` 的居中两端对齐负责撑满高度。
    let thickness = ui
        .text_style_height(&egui::TextStyle::Body)
        .at_least(ui.spacing().interact_size.y);
    let slider_rect = Rect::from_center_size(rect.center(), egui::vec2(thickness, rect.height()));
    let mut value = volume.clamp(0.0, 1.0);
    let response = ui
        .push_id(id_salt, |ui| {
            // 竖向长度取 `spacing.slider_width`（默认 100），这里按推子高度覆盖，
            // 让轨道撑满通道条而不是缩在中间。
            ui.spacing_mut().slider_width = rect.height();
            ui.put(
                slider_rect,
                egui::Slider::new(&mut value, 0.0..=1.0)
                    .vertical()
                    .show_value(false),
            )
        })
        .inner;
    response.changed().then_some(value)
}

/// 绘制一条通道条（音量推子除外，由 [`show_fader`] 绘制）。
fn draw_strip(painter: &egui::Painter, rect: Rect, strip: &MixerStrip, with_buttons: bool) {
    painter.rect(
        rect,
        CornerRadius::same(4),
        theme::card_bg(),
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );
    // 名称（裁剪在条内，长名不会压到相邻通道）。
    painter.with_clip_rect(rect.shrink(2.0)).text(
        egui::pos2(rect.center().x, rect.top() + 8.0),
        Align2::CENTER_TOP,
        &strip.name,
        FontId::proportional(11.0),
        theme::text(),
    );

    // 电平条（右侧），与推子上下对齐。
    let fader = super::fader_rect(rect, with_buttons);
    let meter = Rect::from_min_max(
        egui::pos2(rect.right() - 12.0, fader.top()),
        egui::pos2(rect.right() - 12.0 + METER_WIDTH, fader.bottom()),
    );
    painter.rect_filled(meter, CornerRadius::same(1), theme::base_bg());
    let filled = meter.height() * strip.meter.clamp(0.0, 1.0);
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(meter.left(), meter.bottom() - filled),
            egui::vec2(METER_WIDTH, filled),
        ),
        CornerRadius::same(1),
        theme::success(),
    );

    if with_buttons {
        // 底部三格：声像 / M / S。
        let row_y = rect.bottom() - 34.0;
        let third = rect.width() / 3.0;
        for (i, (label, active)) in [
            ("P", strip.pan.abs() > 0.01),
            ("M", strip.muted),
            ("S", strip.soloed),
        ]
        .into_iter()
        .enumerate()
        {
            let cell = Rect::from_min_size(
                egui::pos2(rect.left() + i as f32 * third + 2.0, row_y),
                egui::vec2(third - 4.0, 18.0),
            );
            painter.rect_filled(
                cell,
                CornerRadius::same(3),
                if active {
                    if label == "M" {
                        theme::danger()
                    } else {
                        theme::warning()
                    }
                } else {
                    theme::toolbar_bg()
                },
            );
            painter.text(
                cell.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(10.0),
                if active {
                    Color32::WHITE
                } else {
                    theme::text_muted()
                },
            );
        }
    } else {
        painter.text(
            egui::pos2(rect.center().x, rect.bottom() - 16.0),
            Align2::CENTER_CENTER,
            "MASTER",
            FontId::proportional(9.0),
            theme::text_muted(),
        );
    }
}

/// 处理本帧按钮 / 滚轮输入，返回动作（矩形与绘制同源）。
///
/// 推子由标准滑杆处理，`hit_test` 不再覆盖推子区域，避免两套输入重复触发。
fn interact(ui: &egui::Ui, layout: &MixerLayout, model: &MixerModel) -> Vec<MixerAction> {
    let (pointer, pressed) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.button_pressed(egui::PointerButton::Primary),
        )
    });
    let mut actions = Vec::new();

    // 滚轮：横向滚动普通通道条（主控条固定）。
    if ui.rect_contains_pointer(layout.panel) {
        let (dx, dy) = ui.input(|i| (i.smooth_scroll_delta.x, i.smooth_scroll_delta.y));
        if dx != 0.0 || dy != 0.0 {
            let max = layout.max_scroll(model.strips.len());
            let next = (model.scroll_x + dx + dy).clamp(0.0, max);
            if (next - model.scroll_x).abs() > f32::EPSILON {
                actions.push(MixerAction::Scrolled(next));
            }
        }
    }

    if pressed
        && let Some(pos) = pointer
        && let Some(hit) = hit_test(pos, layout, model)
    {
        match hit {
            MixerHit::Pan(index) => {
                let strip = layout.strip_rect(index, model.scroll_x);
                let third = strip.width() / 3.0;
                let rel = ((pos.x - strip.left()) / third).clamp(0.0, 1.0);
                actions.push(MixerAction::Pan {
                    index,
                    value: rel * 2.0 - 1.0,
                });
            }
            MixerHit::Mute(index) => actions.push(MixerAction::Mute(index)),
            MixerHit::Solo(index) => actions.push(MixerAction::Solo(index)),
        }
    }
    actions
}
