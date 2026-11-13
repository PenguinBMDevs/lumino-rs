//! 设置「音频」页：输出类型 / 引擎 / 设备 / XSynth 与 LGS 参数 / 系统 MIDI。
//!
//! 行由生产预计算成 `AudioRowModel`，这里只按控件种类绘制并回传字段级动作；
//! 页面内容高于卡片时由调用方套 `ScrollArea`。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::chrome::{AudioControlModel, AudioField, AudioPageModel, SettingsPalette};
use super::super::widgets;
use super::action::{self, SettingsAction};
use super::pages::{action_button, text_width, title};
use super::set_hits;

/// 内容内边距 / 行间距 / 标题区高度（与生产页面一致）。
const PAD: f32 = 20.0;
const GAP: f32 = 10.0;
const TITLE_BLOCK: f32 = 44.0;

/// 页面内容总高度（供 `ScrollArea` 预留空间）。
pub(super) fn content_height(model: &AudioPageModel) -> f32 {
    let rows: f32 = model
        .rows
        .iter()
        .map(|row| row_height(&row.control) + GAP)
        .sum();
    PAD * 2.0 + TITLE_BLOCK + rows
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
    let x = rect.left() + PAD;
    title(ui, x, rect.top(), &model.title, palette);
    let mut y = rect.top() + PAD + TITLE_BLOCK;

    for row in &model.rows {
        let h = row_height(&row.control);
        let center = y + h / 2.0;
        let mut hit: Option<Rect> = None;
        match &row.control {
            AudioControlModel::Checkbox { checked } => {
                hit = Some(checkbox_row(
                    ui, x, y, *checked, &row.label, &row.hint, row.field, palette,
                ));
            }
            AudioControlModel::Button => {
                let width = text_width(ui, &row.label, 14.0) + 24.0;
                let button = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, h));
                hit = Some(button);
                if action_button(ui, button, &row.label, true, palette).clicked() {
                    action::push_action(ui.ctx(), SettingsAction::AudioButton { field: row.field });
                }
            }
            AudioControlModel::Static { text, dim } => {
                draw_label(ui, x, center, &row.label, &row.hint, 0.0, palette);
                let label_w = text_width(ui, &row.label, 14.0);
                ui.painter().text(
                    Pos2::new(x + label_w + 32.0, center),
                    Align2::LEFT_CENTER,
                    text,
                    FontId::proportional(if *dim { 12.0 } else { 14.0 }),
                    if *dim { palette.text_dim } else { palette.text },
                );
            }
            AudioControlModel::Combo { options, selected } => {
                let control_x = rect.right() - PAD - row.width;
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
                let control_x = rect.right() - PAD - row.width;
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
                let control_x = rect.right() - PAD - row.width;
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
                    ui.painter().text(
                        Pos2::new(field.left() + 8.0, field.center().y),
                        Align2::LEFT_CENTER,
                        placeholder,
                        FontId::proportional(14.0),
                        palette.text_dim,
                    );
                }
            }
        }
        if let Some(rect) = hit {
            let mut hits = super::hits(ui.ctx());
            hits.audio_rows.push((row.field, rect));
            set_hits(ui.ctx(), hits);
        }
        y += h + GAP;
    }
}

/// 标签 + 悬浮说明。
fn draw_label(
    ui: &mut egui::Ui,
    x: f32,
    center_y: f32,
    label: &str,
    hint: &str,
    max_width: f32,
    palette: &SettingsPalette,
) {
    // 右侧控件占位固定：长标签必须截断，否则会压到控件上（生产 旧版 用 Fill
    // 约束宽度；egui painter 不做裁剪，这里手动加省略号）。
    let label = truncate(ui, label, max_width);
    ui.painter().text(
        Pos2::new(x, center_y),
        Align2::LEFT_CENTER,
        &label,
        FontId::proportional(14.0),
        palette.text,
    );
    if hint.is_empty() {
        return;
    }
    let width = text_width(ui, &label, 14.0);
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
}

/// 按最大宽度截断标签（尾部加省略号）。
fn truncate(ui: &egui::Ui, text: &str, max_width: f32) -> String {
    if max_width <= 0.0 || text_width(ui, text, 14.0) <= max_width {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate = format!("{}…", chars.iter().collect::<String>());
        if text_width(ui, &candidate, 14.0) <= max_width {
            return candidate;
        }
    }
    "…".to_owned()
}

/// 对钩框 + 标签（点击文字等同切换）。
#[allow(clippy::too_many_arguments)]
fn checkbox_row(
    ui: &mut egui::Ui,
    x: f32,
    y: f32,
    checked: bool,
    label: &str,
    hint: &str,
    field: AudioField,
    palette: &SettingsPalette,
) -> Rect {
    let box_rect = Rect::from_min_size(Pos2::new(x, y + 2.0), Vec2::splat(18.0));
    let label_w = text_width(ui, label, 14.0);
    let label_rect = Rect::from_min_size(
        Pos2::new(box_rect.right() + 8.0, y - 1.0),
        Vec2::new(label_w, 24.0),
    );
    let mut toggled = false;
    let box_response = ui.interact(
        box_rect,
        egui::Id::unique(("audio-check", field as u8)),
        Sense::click(),
    );
    if box_response.clicked() {
        toggled = true;
    }
    let label_response = ui.interact(
        label_rect,
        egui::Id::unique(("audio-check-label", field as u8)),
        Sense::click(),
    );
    if label_response.clicked() {
        toggled = true;
    }
    label_response.on_hover_text(hint);

    ui.painter().rect(
        box_rect,
        CornerRadius::same(4),
        if checked {
            palette.accent
        } else {
            Color32::TRANSPARENT
        },
        Stroke::new(1.0, palette.input_border),
        egui::StrokeKind::Inside,
    );
    if checked {
        let stroke = Stroke::new(1.8, Color32::from_rgb(0x12, 0x1A, 0x2A));
        let p1 = Pos2::new(box_rect.left() + 4.0, box_rect.center().y);
        let p2 = Pos2::new(box_rect.left() + 7.5, box_rect.bottom() - 4.5);
        let p3 = Pos2::new(box_rect.right() - 4.0, box_rect.top() + 4.5);
        ui.painter().line_segment([p1, p2], stroke);
        ui.painter().line_segment([p2, p3], stroke);
    }
    ui.painter().text(
        Pos2::new(label_rect.left(), label_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        palette.text,
    );
    if toggled {
        action::push_action(
            ui.ctx(),
            SettingsAction::AudioToggle {
                field,
                checked: !checked,
            },
        );
    }
    box_rect
}
