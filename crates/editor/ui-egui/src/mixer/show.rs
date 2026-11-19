//! 混音台绘制与交互（对话框内容区）。
//!
//! 所有控件都是 painter 几何绘制；交互结果写入 [`super::MixerAction`] 通道，
//! 由 Runner 在对话框重绘后统一取走并回放到主窗口。

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke};

use super::{
    BODY_PAD, DragTarget, HEADER_H, METER_WIDTH, MixerAction, MixerDialogModel, MixerHit,
    MixerLayout, MixerModel, MixerStrip, drag_target, hit_test, push_action, set_drag_target,
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
    if model.titlebar {
        draw_header(ui, &layout);
    }

    let painter = ui.painter();
    let strip_painter = painter.with_clip_rect(layout.strips_viewport);
    for index in layout.visible(model.mixer.strips.len(), model.mixer.scroll_x) {
        let Some(strip) = model.mixer.strips.get(index) else {
            continue;
        };
        draw_strip(
            &strip_painter,
            layout.strip_rect(index, model.mixer.scroll_x),
            strip,
            true,
        );
    }
    if let (Some(rect), Some(master)) = (layout.master, model.mixer.master.as_ref()) {
        draw_strip(painter, rect, master, false);
    }

    if model.mixer.strips.is_empty() && model.mixer.master.is_none() {
        painter.text(
            layout.body.center(),
            Align2::CENTER_CENTER,
            "暂无音轨",
            FontId::proportional(12.0),
            theme::text_muted(),
        );
    }

    for action in interact(ui, &layout, &model.mixer) {
        push_action(ui.ctx(), action);
    }
}

/// 自制标题栏下的内容标题行：名称 + 操作提示。
fn draw_header(ui: &egui::Ui, layout: &MixerLayout) {
    let rect = Rect::from_min_size(
        egui::pos2(layout.panel.left(), layout.panel.top() + layout::TITLEBAR_H),
        egui::vec2(layout.panel.width(), HEADER_H),
    );
    let painter = ui.painter();
    painter.text(
        egui::pos2(rect.left() + BODY_PAD, rect.center().y),
        Align2::LEFT_CENTER,
        "混音台",
        FontId::proportional(12.0),
        theme::text(),
    );
    painter.text(
        egui::pos2(rect.right() - BODY_PAD, rect.center().y),
        Align2::RIGHT_CENTER,
        "滚轮横向滚动 · 拖动推子调节音量",
        FontId::proportional(10.0),
        theme::text_muted(),
    );
}

/// 绘制一条通道条。
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

    // 推子：轨道 + 位置块。
    let track = Rect::from_min_max(
        egui::pos2(rect.center().x - 3.0, rect.top() + 24.0),
        egui::pos2(
            rect.center().x + 3.0,
            rect.bottom() - if with_buttons { 48.0 } else { 24.0 },
        ),
    );
    painter.rect(
        track,
        CornerRadius::same(2),
        theme::base_bg(),
        Stroke::NONE,
        egui::StrokeKind::Inside,
    );
    let h = track.height() - 6.0;
    let knob_y = track.bottom() - 3.0 - h * strip.volume.clamp(0.0, 1.0);
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(track.left() - 5.0, knob_y - 4.0),
            egui::vec2(track.width() + 10.0, 8.0),
        ),
        CornerRadius::same(2),
        if strip.master {
            theme::accent_strong()
        } else {
            theme::accent()
        },
    );

    // 电平条（右侧）。
    let meter = Rect::from_min_max(
        egui::pos2(rect.right() - 12.0, track.top()),
        egui::pos2(rect.right() - 12.0 + METER_WIDTH, track.bottom()),
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

/// 处理本帧输入，返回动作（矩形与绘制同源）。
fn interact(ui: &egui::Ui, layout: &MixerLayout, model: &MixerModel) -> Vec<MixerAction> {
    let (pointer, pressed, released, down) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.button_pressed(egui::PointerButton::Primary),
            i.pointer.button_released(egui::PointerButton::Primary),
            i.pointer.primary_down(),
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

    if pressed {
        // 任何一次新按下都先清掉旧拖拽目标：丢帧 / 失焦导致 release 丢失时，
        // 这里可以自愈，避免下一次移动意外改到上一条推子。
        set_drag_target(ui.ctx(), None);
        if let Some(pos) = pointer
            && let Some(hit) = hit_test(pos, layout, model)
        {
            match hit {
                MixerHit::Fader(index) => {
                    set_drag_target(ui.ctx(), Some(DragTarget::Track(index)));
                    if let Some(value) = track_value(pos, layout, model, index) {
                        actions.push(MixerAction::Gain { index, value });
                    }
                }
                MixerHit::MasterVolume => {
                    set_drag_target(ui.ctx(), Some(DragTarget::Master));
                    if let Some(value) = master_value(pos, layout) {
                        actions.push(MixerAction::MasterVolume(value));
                    }
                }
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
            return actions;
        }
    }

    if down
        && let Some(pos) = pointer
        && let Some(target) = drag_target(ui.ctx())
    {
        match target {
            DragTarget::Track(index) => {
                if let Some(value) = track_value(pos, layout, model, index) {
                    actions.push(MixerAction::Gain { index, value });
                }
            }
            DragTarget::Master => {
                if let Some(value) = master_value(pos, layout) {
                    actions.push(MixerAction::MasterVolume(value));
                }
            }
        }
        return actions;
    }

    if released {
        set_drag_target(ui.ctx(), None);
    }
    actions
}

/// 指针纵坐标 → 普通通道推子值。
fn track_value(
    pos: egui::Pos2,
    layout: &MixerLayout,
    model: &MixerModel,
    index: usize,
) -> Option<f32> {
    if index >= model.strips.len() {
        return None;
    }
    let strip = layout.strip_rect(index, model.scroll_x);
    let top = strip.top() + 24.0;
    let bottom = strip.bottom() - 48.0;
    Some(((bottom - pos.y) / (bottom - top).max(1.0)).clamp(0.0, 1.0))
}

/// 指针纵坐标 → 主控推子值。
fn master_value(pos: egui::Pos2, layout: &MixerLayout) -> Option<f32> {
    let rect = layout.master?;
    let top = rect.top() + 24.0;
    let bottom = rect.bottom() - 24.0;
    Some(((bottom - pos.y) / (bottom - top).max(1.0)).clamp(0.0, 1.0))
}
