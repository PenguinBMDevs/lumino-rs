//! 设置「界面」页：语言 / 主题卡片 / 字体 / 交互 / 自动滚动。
//!
//! 通用行绘制：下拉、文本、开关、滑杆、按钮、小节标题与主题卡片网格。
//! 主题卡片按 158×96 固定比例自适应列数，点击回传规范主题名。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::chrome::{SettingsPalette, UiField, UiPageModel, UiRow};
use super::super::widgets as egui_widgets;
use super::action::{self, SettingsAction};
use super::pages::{action_button, text_width, title};
use super::set_hits;
use crate::icons::{Icon, IconCache};

mod widgets;
use widgets::{checkbox_row, draw_label, draw_theme_card};

const PAD: f32 = 20.0;
const GAP: f32 = 10.0;
const TITLE_BLOCK: f32 = 44.0;
/// 主题卡片尺寸 / 间距（生产 `CARD_WIDTH` / `CARD_HEIGHT` / `CARD_SPACING`）。
pub(super) const CARD_W: f32 = 158.0;
pub(super) const CARD_H: f32 = 96.0;
pub(super) const CARD_GAP: f32 = 12.0;
/// 卡片内的迷你面板参数。
pub(super) const MOCK_PAD: f32 = 6.0;
pub(super) const BAR_H: f32 = 4.0;

fn row_height(row: &UiRow, width: f32) -> f32 {
    match row {
        UiRow::Section { .. } => 26.0,
        UiRow::Combo { .. } => 32.0,
        UiRow::Text { .. } => 31.0,
        UiRow::Checkbox { .. } => 22.0,
        UiRow::Slider { .. } => 22.0,
        UiRow::Button { .. } => 30.0,
        UiRow::Static { .. } => 18.0,
        UiRow::ThemeCards { cards, .. } => {
            let cols = grid_columns(width);
            let rows = cards.len().div_ceil(cols).max(1) as f32;
            22.0 + rows * CARD_H + (rows - 1.0) * CARD_GAP
        }
        UiRow::Swatches { colors } => {
            let rows = colors.len().div_ceil(12).max(1) as f32;
            20.0 + rows * 28.0 + (rows - 1.0) * 2.0
        }
        UiRow::AboutLogo { size, visible, .. } => {
            if *visible {
                size + 10.0
            } else {
                0.0
            }
        }
        UiRow::EchoCave { .. } => 18.0,
    }
}

fn grid_columns(width: f32) -> usize {
    let usable = (width - PAD * 2.0).max(CARD_W);
    (((usable + CARD_GAP) / (CARD_W + CARD_GAP)).floor() as usize).max(1)
}

/// 页面内容总高度。
pub(super) fn content_height(model: &UiPageModel, width: f32) -> f32 {
    let rows: f32 = model
        .rows
        .iter()
        .map(|row| row_height(row, width) + GAP)
        .sum();
    PAD * 2.0 + TITLE_BLOCK + rows
}

/// 绘制页面。
pub(super) fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &UiPageModel,
    palette: &SettingsPalette,
    icons: &mut IconCache,
) {
    let x = rect.left() + PAD;
    title(ui, x, rect.top(), &model.title, palette);
    let mut y = rect.top() + PAD + TITLE_BLOCK;

    for row in &model.rows {
        let h = row_height(row, rect.width());
        let center = y + h / 2.0;
        let mut hit: Option<(UiField, Rect)> = None;
        match row {
            UiRow::Section { title, hint } => {
                let response = ui.painter().text(
                    Pos2::new(x, y + 13.0),
                    Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(16.0),
                    palette.text,
                );
                if !hint.is_empty() {
                    let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(response.width(), 26.0));
                    ui.interact(
                        r,
                        egui::Id::unique(("ui-section-hint", title.as_str())),
                        Sense::hover(),
                    )
                    .on_hover_text(hint);
                }
            }
            UiRow::Static { text } => {
                ui.painter().text(
                    Pos2::new(x, center),
                    Align2::LEFT_CENTER,
                    text,
                    FontId::proportional(12.0),
                    palette.text_dim,
                );
            }
            UiRow::Combo {
                field,
                label,
                hint,
                options,
                selected,
                width,
            } => {
                let control_x = rect.right() - PAD - width;
                draw_label(ui, x, center, label, hint, control_x - x - 8.0, palette);
                let combo = Rect::from_min_size(Pos2::new(control_x, y), Vec2::new(*width, h));
                hit = Some((*field, combo));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(combo)
                        .layout(Layout::top_down(Align::Min)),
                );
                let current = options.get(*selected).cloned().unwrap_or_default();
                if let Some(index) = egui_widgets::combo_box(
                    &mut child,
                    &format!("ui-combo-{}", *field as u8),
                    &current,
                    options,
                    *width,
                    palette,
                ) {
                    action::push_action(
                        ui.ctx(),
                        SettingsAction::UiCombo {
                            field: *field,
                            index,
                        },
                    );
                }
            }
            UiRow::Checkbox {
                field,
                label,
                hint,
                checked,
            } => {
                hit = Some((
                    *field,
                    checkbox_row(ui, x, y, *checked, label, hint, *field, palette),
                ));
            }
            UiRow::Slider {
                field,
                label,
                hint,
                value,
                min,
                max,
                step,
                value_text,
            } => {
                let control_x = rect.right() - PAD - 180.0 - 48.0;
                draw_label(ui, x, center, label, hint, control_x - x - 8.0, palette);
                let slider = Rect::from_min_size(
                    Pos2::new(control_x, center - 10.0),
                    Vec2::new(180.0, 20.0),
                );
                hit = Some((*field, slider));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(slider)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                child.spacing_mut().slider_width = 162.0;
                let mut v = *value;
                let changed = child
                    .push_id(("ui-slider", *field as u8), |ui| {
                        ui.add(
                            egui::Slider::new(&mut v, *min..=*max)
                                .show_value(false)
                                .step_by(*step),
                        )
                        .changed()
                    })
                    .inner;
                if changed {
                    action::push_action(
                        ui.ctx(),
                        SettingsAction::UiSlider {
                            field: *field,
                            value: v,
                        },
                    );
                }
                ui.painter().text(
                    Pos2::new(rect.right() - PAD, center),
                    Align2::RIGHT_CENTER,
                    value_text,
                    FontId::proportional(14.0),
                    palette.text,
                );
            }
            UiRow::Button { field, label } => {
                let width = text_width(ui, label, 14.0) + 24.0;
                let button = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, h));
                hit = Some((*field, button));
                if action_button(ui, button, label, true, palette).clicked() {
                    action::push_action(ui.ctx(), SettingsAction::UiButton { field: *field });
                }
            }
            UiRow::Text {
                field,
                label,
                hint,
                value,
                placeholder,
                width,
                suffix,
            } => {
                let suffix_w = if suffix.is_empty() {
                    0.0
                } else {
                    text_width(ui, suffix, 12.0) + 8.0
                };
                let control_x = rect.right() - PAD - width - suffix_w;
                draw_label(ui, x, center, label, hint, control_x - x - 8.0, palette);
                let field_rect = Rect::from_min_size(Pos2::new(control_x, y), Vec2::new(*width, h));
                hit = Some((*field, field_rect));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(field_rect)
                        .layout(Layout::top_down(Align::Min)),
                );
                let mut text = value.clone();
                let changed = child
                    .push_id(("ui-text", *field as u8), |ui| {
                        egui_widgets::numeric_input(ui, &mut text, *width, palette).changed()
                    })
                    .inner;
                if changed {
                    action::push_action(
                        ui.ctx(),
                        SettingsAction::UiText {
                            field: *field,
                            value: text.clone(),
                        },
                    );
                }
                if !placeholder.is_empty() && text.is_empty() {
                    ui.painter().text(
                        Pos2::new(field_rect.left() + 8.0, field_rect.center().y),
                        Align2::LEFT_CENTER,
                        placeholder,
                        FontId::proportional(14.0),
                        palette.text_dim,
                    );
                }
                if !suffix.is_empty() {
                    ui.painter().text(
                        Pos2::new(field_rect.right() + 8.0, field_rect.center().y),
                        Align2::LEFT_CENTER,
                        suffix,
                        FontId::proportional(12.0),
                        palette.text_dim,
                    );
                }
            }
            UiRow::ThemeCards { label, cards } => {
                ui.painter().text(
                    Pos2::new(x, y + 11.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(14.0),
                    palette.text,
                );
                let top = y + 22.0;
                let cols = grid_columns(rect.width());
                for (index, card) in cards.iter().enumerate() {
                    let col = index % cols;
                    let row = index / cols;
                    let card_rect = Rect::from_min_size(
                        Pos2::new(
                            x + col as f32 * (CARD_W + CARD_GAP),
                            top + row as f32 * (CARD_H + CARD_GAP),
                        ),
                        Vec2::new(CARD_W, CARD_H),
                    );
                    draw_theme_card(ui, card_rect, card);
                    let response = ui.interact(
                        card_rect,
                        egui::Id::unique(("ui-theme-card", card.value.as_str())),
                        Sense::click(),
                    );
                    if response.clicked() {
                        action::push_action(ui.ctx(), SettingsAction::UiTheme(card.value.clone()));
                    }
                    let mut hits = super::hits(ui.ctx());
                    hits.theme_cards.push((card.value.clone(), card_rect));
                    set_hits(ui.ctx(), hits);
                }
            }
            UiRow::Swatches { colors } => {
                let panel =
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(rect.width() - PAD * 2.0, h));
                ui.painter().rect(
                    panel,
                    CornerRadius::same(21),
                    palette.toolbar,
                    Stroke::new(1.0, palette.input_border),
                    egui::StrokeKind::Inside,
                );
                let start = panel.min + Vec2::splat(10.0);
                for (index, color) in colors.iter().enumerate() {
                    let col = index % 12;
                    let row = index / 12;
                    let swatch = Rect::from_min_size(
                        Pos2::new(start.x + col as f32 * 30.0, start.y + row as f32 * 30.0),
                        Vec2::splat(28.0),
                    );
                    ui.painter().rect(
                        swatch,
                        CornerRadius::same(3),
                        *color,
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 38)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            UiRow::AboutLogo { dx, size, visible } => {
                if *visible {
                    let logo = Rect::from_min_size(Pos2::new(x + 7.0 + dx, y), Vec2::splat(*size));
                    icons.paint(ui.painter(), Icon::LogoInApp, logo);
                    hit = Some((UiField::AboutLogoClicked, logo));
                    let response = ui.interact(
                        logo.expand(7.0),
                        egui::Id::unique("ui-about-logo"),
                        Sense::click(),
                    );
                    if response.clicked() {
                        action::push_action(
                            ui.ctx(),
                            SettingsAction::UiButton {
                                field: UiField::AboutLogoClicked,
                            },
                        );
                    }
                }
            }
            UiRow::EchoCave { text, opacity } => {
                let row =
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(rect.width() - PAD * 2.0, 18.0));
                let color = Color32::from_rgba_unmultiplied(
                    palette.text.r(),
                    palette.text.g(),
                    palette.text.b(),
                    (255.0 * opacity.clamp(0.0, 1.0)).round() as u8,
                );
                ui.painter().text(
                    Pos2::new(row.left(), row.center().y),
                    Align2::LEFT_CENTER,
                    text,
                    FontId::proportional(12.0),
                    color,
                );
                hit = Some((UiField::EchoCaveClicked, row));
                let response = ui.interact(row, egui::Id::unique("ui-echo-cave"), Sense::click());
                if response.clicked() {
                    action::push_action(
                        ui.ctx(),
                        SettingsAction::UiButton {
                            field: UiField::EchoCaveClicked,
                        },
                    );
                }
            }
        }
        if let Some((field, rect)) = hit {
            let mut hits = super::hits(ui.ctx());
            hits.ui_rows.push((field, rect));
            set_hits(ui.ctx(), hits);
        }
        y += h + GAP;
    }
}
