//! 设置「音频」页：输出类型 / 引擎 / 设备 / XSynth 与 LGS 参数 / 系统 MIDI。
//!
//! 行由生产预计算成 `AudioRowModel`，这里只按控件种类绘制并回传字段级动作；
//! 页面内容高于卡片时由调用方套 `ScrollArea`。

use egui::{Align, Align2, FontId, Layout, Pos2, Rect, Sense, Vec2};

use super::super::chrome::{AudioControlModel, AudioPageModel, AudioRowModel, SettingsPalette};
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

/// 一行控件的公共绘制上下文（几何 / 行模型 / 调色板）。
struct RowCtx<'a> {
    x: f32,
    y: f32,
    h: f32,
    center: f32,
    right: f32,
    track_w: f32,
    row: &'a AudioRowModel,
    palette: &'a SettingsPalette,
}

/// 标签的绘制参数。
struct LabelCtx<'a> {
    x: f32,
    center_y: f32,
    label: &'a str,
    hint: &'a str,
    max_width: f32,
    palette: &'a SettingsPalette,
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
        let ctx = RowCtx {
            x,
            y,
            h,
            center: cursor.center(h),
            right: cursor.right(),
            track_w: cursor.control_width(crate::widgets::TRACK_W),
            row,
            palette,
        };
        let hit = match &row.control {
            AudioControlModel::Checkbox { checked } => draw_checkbox(ui, &ctx, *checked),
            AudioControlModel::Button => draw_button(ui, &ctx, cursor.control_width(1.0)),
            AudioControlModel::Static { text, dim } => {
                draw_static(ui, &ctx, cursor.control_width(1.0), text, *dim);
                None
            }
            AudioControlModel::Combo { options, selected } => {
                draw_combo(ui, &ctx, options, *selected)
            }
            AudioControlModel::Slider {
                value,
                min,
                max,
                step,
            } => draw_slider(ui, &ctx, *value, *min, *max, *step),
            AudioControlModel::Text { value, placeholder } => {
                draw_text(ui, &ctx, value, placeholder)
            }
        };
        if let Some(rect) = hit {
            let mut hits = super::hits(ui.ctx());
            hits.audio_rows.push((row.field, rect));
            set_hits(ui.ctx(), hits);
        }
        cursor.advance(h);
    }
}

/// 复选框行；返回需要登记命中的轨道矩形。
fn draw_checkbox(ui: &mut egui::Ui, ctx: &RowCtx<'_>, checked: bool) -> Option<Rect> {
    let row_rect = Rect::from_min_size(Pos2::new(ctx.x, ctx.y), Vec2::new(ctx.track_w, ctx.h));
    let switch = crate::widgets::switch_row(
        ui,
        row_rect,
        &crate::widgets::SwitchRow {
            id: egui::Id::unique(("audio-switch", ctx.row.field as u8)),
            checked,
            enabled: true,
            label: &ctx.row.label,
            hint: &ctx.row.hint,
        },
        &switch_style(ctx.palette),
    );
    if switch.toggled {
        action::push_action(
            ui.ctx(),
            SettingsAction::AudioToggle {
                field: ctx.row.field,
                checked: !checked,
            },
        );
    }
    Some(switch.track)
}

/// 按钮行；返回需要登记命中的按钮矩形。
fn draw_button(ui: &mut egui::Ui, ctx: &RowCtx<'_>, available: f32) -> Option<Rect> {
    let width = (text_width(ui, &ctx.row.label, 14.0) + 24.0).min(available);
    let button = Rect::from_min_size(Pos2::new(ctx.x, ctx.y), Vec2::new(width, ctx.h));
    let hit = Some(button);
    if action_button(ui, button, &ctx.row.label, true, ctx.palette).clicked() {
        action::push_action(
            ui.ctx(),
            SettingsAction::AudioButton {
                field: ctx.row.field,
            },
        );
    }
    hit
}

/// 只读文本行（无命中区）。
fn draw_static(ui: &mut egui::Ui, ctx: &RowCtx<'_>, available: f32, text: &str, dim: bool) {
    let label_w = draw_label(
        ui,
        &LabelCtx {
            x: ctx.x,
            center_y: ctx.center,
            label: &ctx.row.label,
            hint: &ctx.row.hint,
            max_width: (available - 112.0).max(1.0),
            palette: ctx.palette,
        },
    );
    crate::text::paint_truncated(
        ui,
        Pos2::new(ctx.x + label_w + 32.0, ctx.center),
        Align2::LEFT_CENTER,
        text,
        if dim { 12.0 } else { 14.0 },
        if dim {
            ctx.palette.text_dim
        } else {
            ctx.palette.text
        },
        (available - label_w - 32.0).max(1.0),
    );
}

/// 下拉框行；返回需要登记命中的控件矩形。
fn draw_combo(
    ui: &mut egui::Ui,
    ctx: &RowCtx<'_>,
    options: &[String],
    selected: usize,
) -> Option<Rect> {
    let control_x = ctx.right - ctx.row.width;
    draw_label(
        ui,
        &LabelCtx {
            x: ctx.x,
            center_y: ctx.center,
            label: &ctx.row.label,
            hint: &ctx.row.hint,
            max_width: control_x - ctx.x - 8.0,
            palette: ctx.palette,
        },
    );
    let combo = Rect::from_min_size(Pos2::new(control_x, ctx.y), Vec2::new(ctx.row.width, ctx.h));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(combo)
            .layout(Layout::top_down(Align::Min)),
    );
    let current = options.get(selected).cloned().unwrap_or_default();
    if let Some(index) = widgets::combo_box(
        &mut child,
        &format!("audio-combo-{}", ctx.row.field as u8),
        &current,
        options,
        ctx.row.width,
        ctx.palette,
    ) {
        action::push_action(
            ui.ctx(),
            SettingsAction::AudioCombo {
                field: ctx.row.field,
                index,
            },
        );
    }
    Some(combo)
}

/// 滑杆行；返回需要登记命中的滑杆矩形。
fn draw_slider(
    ui: &mut egui::Ui,
    ctx: &RowCtx<'_>,
    value: f32,
    min: f32,
    max: f32,
    step: f64,
) -> Option<Rect> {
    let control_x = ctx.right - ctx.row.width;
    draw_label(
        ui,
        &LabelCtx {
            x: ctx.x,
            center_y: ctx.center,
            label: &ctx.row.label,
            hint: &ctx.row.hint,
            max_width: control_x - ctx.x - 8.0,
            palette: ctx.palette,
        },
    );
    let slider = Rect::from_min_size(
        Pos2::new(control_x, ctx.center - 10.0),
        Vec2::new(ctx.row.width, 20.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(slider)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().slider_width = ctx.row.width - 18.0;
    let mut v = value;
    // 稳定 ID：切换后端时页面重排，自动 ID 可能与旧弹层项碰撞，
    // 导致新位置的滑杆被上一次点击误命中。
    let changed = child
        .push_id(("audio-slider", ctx.row.field as u8), |ui| {
            let response = ui.add(
                egui::Slider::new(&mut v, min..=max)
                    .show_value(false)
                    .step_by(step),
            );
            response.changed()
        })
        .inner;
    if changed {
        action::push_action(
            ui.ctx(),
            SettingsAction::AudioSlider {
                field: ctx.row.field,
                value: v,
            },
        );
    }
    Some(slider)
}

/// 文本输入行；返回需要登记命中的输入框矩形。
fn draw_text(ui: &mut egui::Ui, ctx: &RowCtx<'_>, value: &str, placeholder: &str) -> Option<Rect> {
    let control_x = ctx.right - ctx.row.width;
    draw_label(
        ui,
        &LabelCtx {
            x: ctx.x,
            center_y: ctx.center,
            label: &ctx.row.label,
            hint: &ctx.row.hint,
            max_width: control_x - ctx.x - 8.0,
            palette: ctx.palette,
        },
    );
    let field = Rect::from_min_size(Pos2::new(control_x, ctx.y), Vec2::new(ctx.row.width, ctx.h));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(field)
            .layout(Layout::top_down(Align::Min)),
    );
    let mut text = value.to_owned();
    // 稳定 ID：与滑杆同理，避免页面重排后与旧弹层点击状态碰撞。
    let changed = child
        .push_id(("audio-text", ctx.row.field as u8), |ui| {
            widgets::numeric_input(ui, &mut text, ctx.row.width, ctx.palette).changed()
        })
        .inner;
    if changed {
        action::push_action(
            ui.ctx(),
            SettingsAction::AudioText {
                field: ctx.row.field,
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
            ctx.palette.text_dim,
            (field.width() - 16.0).max(1.0),
        );
    }
    Some(field)
}

/// 标签 + 悬浮说明；返回实际绘制的标签宽度。
fn draw_label(ui: &mut egui::Ui, ctx: &LabelCtx<'_>) -> f32 {
    // 右侧控件占位固定：长标签必须截断，否则会压到控件上（生产 上一代 用 Fill
    // 约束宽度；egui painter 不做裁剪，这里手动加省略号）。
    let label = widgets::truncate_to_width(ui, ctx.label, 14.0, ctx.max_width.max(1.0));
    ui.painter().text(
        Pos2::new(ctx.x, ctx.center_y),
        Align2::LEFT_CENTER,
        &label,
        FontId::proportional(14.0),
        ctx.palette.text,
    );
    let width = text_width(ui, &label, 14.0);
    if ctx.hint.is_empty() {
        return width;
    }
    let rect = Rect::from_min_size(
        Pos2::new(ctx.x, ctx.center_y - 10.0),
        Vec2::new(width.max(8.0), 20.0),
    );
    let response = ui.interact(
        rect,
        egui::Id::unique(("audio-label-hint", label.as_str())),
        Sense::hover(),
    );
    response.on_hover_text(ctx.hint);
    width
}
