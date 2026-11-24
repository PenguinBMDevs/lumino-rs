//! 设置「音频」页：输出类型 / 引擎 / 设备 / XSynth 与 LGS 参数 / 系统 MIDI。
//!
//! 行由生产预计算成 `AudioRowModel`，这里只按控件种类绘制并回传字段级动作；
//! 页面内容高于卡片时由调用方套 `ScrollArea`。

use egui::{Align, Align2, FontId, Layout, Pos2, Rect, Sense, Vec2};

use super::super::chrome::{AudioControlModel, AudioPageModel, SettingsPalette};
use super::super::widgets;
use super::action::{self, SettingsAction};
use super::engine::{self, RowCursor};
use super::pages::{action_button, switch_style, title};
use super::set_hits;
use crate::widgets::text_width;

/// 页面内容总高度（供 `ScrollArea` 预留空间）。
pub(super) fn content_height(model: &AudioPageModel) -> f32 {
    engine::content_height(model.rows.iter().map(|row| row_height(&row.control)))
}

fn row_height(control: &AudioControlModel) -> f32 {
    match control {
        AudioControlModel::Combo { .. } => 32.0,
        AudioControlModel::Slider { .. } => 22.0,
        AudioControlModel::Text { .. } => 31.0,
        AudioControlModel::Checkbox { .. } => 22.0,
        AudioControlModel::Button => 30.0,
        AudioControlModel::Static { .. } => 18.0,
    }
}

/// 绘制音频页。
pub(super) fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &AudioPageModel,
    palette: &SettingsPalette,
) {
    let mut cursor = RowCursor::new(rect);
    let x = cursor.x();
    title(ui, x, rect.top(), &model.title, palette);

    for row in &model.rows {
        let y = cursor.y();
        let h = row_height(&row.control);
        let center = cursor.center(h);
        let mut hit: Option<Rect> = None;
        match &row.control {
            AudioControlModel::Checkbox { checked } => {
                let row_rect = Rect::from_min_size(
                    Pos2::new(x, y),
                    Vec2::new(cursor.control_width(crate::widgets::TRACK_W), h),
                );
                let switch = crate::widgets::switch_row(
                    ui,
                    row_rect,
                    &crate::widgets::SwitchRow {
                        id: egui::Id::unique(("audio-switch", row.field as u8)),
                        checked: *checked,
                        enabled: true,
                        label: &row.label,
                        hint: &row.hint,
                    },
                    &switch_style(palette),
                );
                if switch.toggled {
                    action::push_action(
                        ui.ctx(),
                        SettingsAction::AudioToggle {
                            field: row.field,
                            checked: !*checked,
                        },
                    );
                }
                hit = Some(switch.track);
            }
            AudioControlModel::Button => {
                let width =
                    (text_width(ui, &row.label, 14.0) + 24.0).min(cursor.control_width(1.0));
                let button = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, h));
                hit = Some(button);
                if action_button(ui, button, &row.label, true, palette).clicked() {
                    action::push_action(ui.ctx(), SettingsAction::AudioButton { field: row.field });
                }
            }
            AudioControlModel::Static { text, dim } => {
                let available = cursor.control_width(1.0);
                let label_w = draw_label(
                    ui,
                    x,
                    center,
                    &row.label,
                    &row.hint,
                    (available - 112.0).max(1.0),
                    palette,
                );
                crate::text::paint_truncated(
                    ui,
                    Pos2::new(x + label_w + 32.0, center),
                    Align2::LEFT_CENTER,
                    text,
                    if *dim { 12.0 } else { 14.0 },
                    if *dim { palette.text_dim } else { palette.text },
                    (available - label_w - 32.0).max(1.0),
                );
            }
            AudioControlModel::Combo { options, selected } => {
                let control_x = cursor.right() - row.width;
                draw_label(
                    ui,
                    x,
                    center,
                    &row.label,
                    &row.hint,
                    control_x - x - 8.0,
                    palette,
                );
                let combo = Rect::from_min_size(Pos2::new(control_x, y), Vec2::new(row.width, h));
                hit = Some(combo);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(combo)
                        .layout(Layout::top_down(Align::Min)),
                );
                let current = options.get(*selected).cloned().unwrap_or_default();
                if let Some(index) = widgets::combo_box(
                    &mut child,
                    &format!("audio-combo-{}", row.field as u8),
                    &current,
                    options,
                    row.width,
                    palette,
                ) {
                    action::push_action(
                        ui.ctx(),
                        SettingsAction::AudioCombo {
                            field: row.field,
                            index,
                        },
                    );
                }
            }
            AudioControlModel::Slider {
                value,
                min,
                max,
                step,
            } => {
                let control_x = cursor.right() - row.width;
                draw_label(
                    ui,
                    x,
                    center,
                    &row.label,
                    &row.hint,
                    control_x - x - 8.0,
                    palette,
                );
                let slider = Rect::from_min_size(
                    Pos2::new(control_x, center - 10.0),
                    Vec2::new(row.width, 20.0),
                );
                hit = Some(slider);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(slider)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                child.spacing_mut().slider_width = row.width - 18.0;
                let mut v = *value;
                // 稳定 ID：切换后端时页面重排，自动 ID 可能与旧弹层项碰撞，
                // 导致新位置的滑杆被上一次点击误命中。
                let changed = child
                    .push_id(("audio-slider", row.field as u8), |ui| {
                        let response = ui.add(
                            egui::Slider::new(&mut v, *min..=*max)
                                .show_value(false)
                                .step_by(*step),
                        );
                        response.changed()
                    })
                    .inner;
                if changed {
                    action::push_action(
                        ui.ctx(),
                        SettingsAction::AudioSlider {
                            field: row.field,
                            value: v,
                        },
                    );
                }
            }
            AudioControlModel::Text { value, placeholder } => {
                let control_x = cursor.right() - row.width;
                draw_label(
                    ui,
                    x,
                    center,
                    &row.label,
                    &row.hint,
                    control_x - x - 8.0,
                    palette,
                );
                let field = Rect::from_min_size(Pos2::new(control_x, y), Vec2::new(row.width, h));
                hit = Some(field);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(field)
                        .layout(Layout::top_down(Align::Min)),
                );
                let mut text = value.clone();
                // 稳定 ID：与滑杆同理，避免页面重排后与旧弹层点击状态碰撞。
                let changed = child
                    .push_id(("audio-text", row.field as u8), |ui| {
                        widgets::numeric_input(ui, &mut text, row.width, palette).changed()
                    })
                    .inner;
                if changed {
                    action::push_action(
                        ui.ctx(),
                        SettingsAction::AudioText {
                            field: row.field,
                            value: text.clone(),
                        },
                    );
                }
                if !placeholder.is_empty() && text.is_empty() {
                    crate::text::paint_truncated(
                        ui,
                        Pos2::new(field.left() + 8.0, field.center().y),
                        Align2::LEFT_CENTER,
                        placeholder,
                        14.0,
                        palette.text_dim,
                        (field.width() - 16.0).max(1.0),
                    );
                }
            }
        }
        if let Some(rect) = hit {
            let mut hits = super::hits(ui.ctx());
            hits.audio_rows.push((row.field, rect));
            set_hits(ui.ctx(), hits);
        }
        cursor.advance(h);
    }
}

/// 标签 + 悬浮说明；返回实际绘制的标签宽度。
fn draw_label(
    ui: &mut egui::Ui,
    x: f32,
    center_y: f32,
    label: &str,
    hint: &str,
    max_width: f32,
    palette: &SettingsPalette,
) -> f32 {
    // 右侧控件占位固定：长标签必须截断，否则会压到控件上（生产 上一代 用 Fill
    // 约束宽度；egui painter 不做裁剪，这里手动加省略号）。
    let label = widgets::truncate_to_width(ui, label, 14.0, max_width.max(1.0));
    ui.painter().text(
        Pos2::new(x, center_y),
        Align2::LEFT_CENTER,
        &label,
        FontId::proportional(14.0),
        palette.text,
    );
    let width = text_width(ui, &label, 14.0);
    if hint.is_empty() {
        return width;
    }
    let rect = Rect::from_min_size(
        Pos2::new(x, center_y - 10.0),
        Vec2::new(width.max(8.0), 20.0),
    );
    let response = ui.interact(
        rect,
        egui::Id::unique(("audio-label-hint", label.as_str())),
        Sense::hover(),
    );
    response.on_hover_text(hint);
    width
}
