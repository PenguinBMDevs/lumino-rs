//! 设置「界面」页：语言 / 主题卡片 / 字体 / 交互 / 自动滚动。
//!
//! 通用行绘制：下拉、文本、开关、滑杆、按钮、小节标题与主题卡片网格。
//! 主题卡片按 158×96 固定比例自适应列数，点击回传规范主题名。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::chrome::{SettingsPalette, ThemeCardModel, UiField, UiPageModel, UiRow};
use super::super::widgets as egui_widgets;
use super::action::{self, SettingsAction};
use super::engine::{self, RowCursor};
use super::pages::{action_button, title};
use super::set_hits;
use crate::icons::{Icon, IconCache};
use crate::widgets::text_width;

mod widgets;
use widgets::{draw_label, draw_theme_card, grid_columns, row_height};

/// 主题卡片尺寸 / 间距（生产 `CARD_WIDTH` / `CARD_HEIGHT` / `CARD_SPACING`）。
pub(super) const CARD_W: f32 = 158.0;
pub(super) const CARD_H: f32 = 96.0;
pub(super) const CARD_GAP: f32 = 12.0;
/// 卡片内的迷你面板参数。
pub(super) const MOCK_PAD: f32 = 6.0;
pub(super) const BAR_H: f32 = 4.0;

/// 页面内容总高度。
pub(super) fn content_height(ui: &egui::Ui, model: &UiPageModel, width: f32) -> f32 {
    engine::content_height(model.rows.iter().map(|row| row_height(ui, row, width)))
}

/// 单行绘制上下文：把游标、坐标与调色板收在一处，避免每个区块重复传参。
struct RowCtx<'a> {
    ui: &'a mut egui::Ui,
    palette: &'a SettingsPalette,
    icons: &'a mut IconCache,
    cursor: &'a RowCursor,
    x: f32,
    y: f32,
    h: f32,
    text_max: f32,
    rect_w: f32,
}

impl RowCtx<'_> {
    fn center(&self) -> f32 {
        self.cursor.center(self.h)
    }
}

/// 绘制页面。
pub(super) fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &UiPageModel,
    palette: &SettingsPalette,
    icons: &mut IconCache,
) {
    let mut cursor = RowCursor::new(rect);
    let x = cursor.x();
    title(ui, x, rect.top(), &model.title, palette);

    for row in &model.rows {
        let y = cursor.y();
        let h = row_height(ui, row, rect.width());
        let text_max = cursor.content_width();
        let hit = {
            let mut ctx = RowCtx {
                ui: &mut *ui,
                palette,
                icons: &mut *icons,
                cursor: &cursor,
                x,
                y,
                h,
                text_max,
                rect_w: rect.width(),
            };
            row_view(&mut ctx, row)
        };
        if let Some((field, rect)) = hit {
            let mut hits = super::hits(ui.ctx());
            hits.ui_rows.push((field, rect));
            set_hits(ui.ctx(), hits);
        }
        cursor.advance(h);
    }
}

/// 按行类型分发；返回需要登记命中区的字段。
fn row_view(ctx: &mut RowCtx<'_>, row: &UiRow) -> Option<(UiField, Rect)> {
    match row {
        UiRow::Section { title, hint } => {
            section_row(ctx, title, hint);
            None
        }
        UiRow::Static { text } => {
            static_row(ctx, text);
            None
        }
        UiRow::Combo {
            field,
            label,
            hint,
            options,
            selected,
            width,
        } => combo_row(ctx, *field, label, hint, options, *selected, *width),
        UiRow::Checkbox {
            field,
            label,
            hint,
            checked,
        } => checkbox_row(ctx, *field, label, hint, *checked),
        UiRow::Slider {
            field,
            label,
            hint,
            value,
            min,
            max,
            step,
            value_text,
        } => slider_row(
            ctx,
            SliderRow {
                field: *field,
                label,
                hint,
                value: *value,
                min: *min,
                max: *max,
                step: *step,
                value_text,
            },
        ),
        UiRow::Button { field, label } => button_row(ctx, *field, label),
        UiRow::Text {
            field,
            label,
            hint,
            value,
            placeholder,
            width,
            suffix,
        } => text_row(
            ctx,
            TextRow {
                field: *field,
                label,
                hint,
                value,
                placeholder,
                width: *width,
                suffix,
            },
        ),
        UiRow::ThemeCards { label, cards } => {
            theme_cards_row(ctx, label, cards);
            None
        }
        UiRow::Swatches { colors } => {
            swatches_row(ctx, colors);
            None
        }
        UiRow::AboutLogo { dx, size, visible } => about_logo_row(ctx, *dx, *size, *visible),
        UiRow::EchoCave { text, opacity } => echo_cave_row(ctx, text, *opacity),
    }
}

/// 小节标题 + 可选悬停说明。
fn section_row(ctx: &mut RowCtx<'_>, title: &str, hint: &str) {
    let label = crate::text::truncate_to_width(ctx.ui, title, 16.0, ctx.text_max);
    let response = ctx.ui.painter().text(
        Pos2::new(ctx.x, ctx.y + 13.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(16.0),
        ctx.palette.text,
    );
    if !hint.is_empty() {
        let r = Rect::from_min_size(Pos2::new(ctx.x, ctx.y), Vec2::new(response.width(), 26.0));
        ctx.ui
            .interact(
                r,
                egui::Id::unique(("ui-section-hint", title)),
                Sense::hover(),
            )
            .on_hover_text(hint);
    }
}

/// 说明性文本：按卡片宽度折行（行高已在 `row_height` 里算入）。
fn static_row(ctx: &mut RowCtx<'_>, text: &str) {
    // 单行时在 18px 行槽里垂直居中，多行时从行顶开始。
    let text_h = crate::text::wrapped_height(ctx.ui, text, 12.0, ctx.text_max);
    let top = ctx.y + ((ctx.h - text_h) * 0.5).max(0.0);
    crate::text::paint_wrapped(
        ctx.ui,
        Pos2::new(ctx.x, top),
        text,
        12.0,
        ctx.palette.text_dim,
        ctx.text_max,
    );
}

/// 下拉框。
fn combo_row(
    ctx: &mut RowCtx<'_>,
    field: UiField,
    label: &str,
    hint: &str,
    options: &[String],
    selected: usize,
    width: f32,
) -> Option<(UiField, Rect)> {
    let control_x = ctx.cursor.right() - width;
    draw_label(
        ctx.ui,
        ctx.x,
        ctx.center(),
        label,
        hint,
        control_x - ctx.x - 8.0,
        ctx.palette,
    );
    let combo = Rect::from_min_size(Pos2::new(control_x, ctx.y), Vec2::new(width, ctx.h));
    let mut child = ctx.ui.new_child(
        egui::UiBuilder::new()
            .max_rect(combo)
            .layout(Layout::top_down(Align::Min)),
    );
    let current = options.get(selected).cloned().unwrap_or_default();
    if let Some(index) = egui_widgets::combo_box(
        &mut child,
        &format!("ui-combo-{}", field as u8),
        &current,
        options,
        width,
        ctx.palette,
    ) {
        action::push_action(ctx.ui.ctx(), SettingsAction::UiCombo { field, index });
    }
    Some((field, combo))
}

/// 开关行。
fn checkbox_row(
    ctx: &mut RowCtx<'_>,
    field: UiField,
    label: &str,
    hint: &str,
    checked: bool,
) -> Option<(UiField, Rect)> {
    let row_rect = Rect::from_min_size(
        Pos2::new(ctx.x, ctx.y),
        Vec2::new(ctx.cursor.control_width(crate::widgets::TRACK_W), ctx.h),
    );
    let switch = widgets::switch_row(ctx.ui, row_rect, field, label, hint, checked, ctx.palette);
    if switch.toggled {
        action::push_action(
            ctx.ui.ctx(),
            SettingsAction::UiToggle {
                field,
                checked: !checked,
            },
        );
    }
    Some((field, switch.track))
}

/// 滑杆行所需的字段（避免单函数超过参数上限）。
struct SliderRow<'a> {
    field: UiField,
    label: &'a str,
    hint: &'a str,
    value: f32,
    min: f32,
    max: f32,
    step: f64,
    value_text: &'a str,
}

/// 滑杆行。
fn slider_row(ctx: &mut RowCtx<'_>, row: SliderRow<'_>) -> Option<(UiField, Rect)> {
    let control_x = ctx.cursor.right() - 180.0 - 48.0;
    draw_label(
        ctx.ui,
        ctx.x,
        ctx.center(),
        row.label,
        row.hint,
        control_x - ctx.x - 8.0,
        ctx.palette,
    );
    let slider = Rect::from_min_size(
        Pos2::new(control_x, ctx.center() - 10.0),
        Vec2::new(180.0, 20.0),
    );
    let mut child = ctx.ui.new_child(
        egui::UiBuilder::new()
            .max_rect(slider)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().slider_width = 162.0;
    let mut v = row.value;
    let changed = child
        .push_id(("ui-slider", row.field as u8), |ui| {
            ui.add(
                egui::Slider::new(&mut v, row.min..=row.max)
                    .show_value(false)
                    .step_by(row.step),
            )
            .changed()
        })
        .inner;
    if changed {
        action::push_action(
            ctx.ui.ctx(),
            SettingsAction::UiSlider {
                field: row.field,
                value: v,
            },
        );
    }
    ctx.ui.painter().text(
        Pos2::new(ctx.cursor.right(), ctx.center()),
        Align2::RIGHT_CENTER,
        row.value_text,
        FontId::proportional(14.0),
        ctx.palette.text,
    );
    Some((row.field, slider))
}

/// 按钮行。
fn button_row(ctx: &mut RowCtx<'_>, field: UiField, label: &str) -> Option<(UiField, Rect)> {
    let width = (text_width(ctx.ui, label, 14.0) + 24.0).min(ctx.text_max);
    let button = Rect::from_min_size(Pos2::new(ctx.x, ctx.y), Vec2::new(width, ctx.h));
    if action_button(ctx.ui, button, label, true, ctx.palette).clicked() {
        action::push_action(ctx.ui.ctx(), SettingsAction::UiButton { field });
    }
    Some((field, button))
}

/// 文本输入行所需的字段。
struct TextRow<'a> {
    field: UiField,
    label: &'a str,
    hint: &'a str,
    value: &'a str,
    placeholder: &'a str,
    width: f32,
    suffix: &'a str,
}

/// 文本输入行。
fn text_row(ctx: &mut RowCtx<'_>, row: TextRow<'_>) -> Option<(UiField, Rect)> {
    let suffix_w = if row.suffix.is_empty() {
        0.0
    } else {
        text_width(ctx.ui, row.suffix, 12.0) + 8.0
    };
    let control_x = ctx.cursor.right() - row.width - suffix_w;
    draw_label(
        ctx.ui,
        ctx.x,
        ctx.center(),
        row.label,
        row.hint,
        control_x - ctx.x - 8.0,
        ctx.palette,
    );
    let field_rect = Rect::from_min_size(Pos2::new(control_x, ctx.y), Vec2::new(row.width, ctx.h));
    let mut child = ctx.ui.new_child(
        egui::UiBuilder::new()
            .max_rect(field_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    let mut text = row.value.to_owned();
    let changed = child
        .push_id(("ui-text", row.field as u8), |ui| {
            egui_widgets::numeric_input(ui, &mut text, row.width, ctx.palette).changed()
        })
        .inner;
    if changed {
        action::push_action(
            ctx.ui.ctx(),
            SettingsAction::UiText {
                field: row.field,
                value: text.clone(),
            },
        );
    }
    if !row.placeholder.is_empty() && text.is_empty() {
        crate::text::paint_truncated(
            ctx.ui,
            Pos2::new(field_rect.left() + 8.0, field_rect.center().y),
            Align2::LEFT_CENTER,
            row.placeholder,
            14.0,
            ctx.palette.text_dim,
            (field_rect.width() - 16.0).max(1.0),
        );
    }
    if !row.suffix.is_empty() {
        ctx.ui.painter().text(
            Pos2::new(field_rect.right() + 8.0, field_rect.center().y),
            Align2::LEFT_CENTER,
            row.suffix,
            FontId::proportional(12.0),
            ctx.palette.text_dim,
        );
    }
    Some((row.field, field_rect))
}

/// 主题卡片网格：点击回传规范主题名。
fn theme_cards_row(ctx: &mut RowCtx<'_>, label: &str, cards: &[ThemeCardModel]) {
    let label = crate::text::truncate_to_width(ctx.ui, label, 14.0, ctx.text_max);
    ctx.ui.painter().text(
        Pos2::new(ctx.x, ctx.y + 11.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        ctx.palette.text,
    );
    let top = ctx.y + 22.0;
    let cols = grid_columns(ctx.rect_w);
    for (index, card) in cards.iter().enumerate() {
        let col = index % cols;
        let row = index / cols;
        let card_rect = Rect::from_min_size(
            Pos2::new(
                ctx.x + col as f32 * (CARD_W + CARD_GAP),
                top + row as f32 * (CARD_H + CARD_GAP),
            ),
            Vec2::new(CARD_W, CARD_H),
        );
        draw_theme_card(ctx.ui, card_rect, card);
        let response = ctx.ui.interact(
            card_rect,
            egui::Id::unique(("ui-theme-card", card.value.as_str())),
            Sense::click(),
        );
        if response.clicked() {
            action::push_action(ctx.ui.ctx(), SettingsAction::UiTheme(card.value.clone()));
        }
        let mut hits = super::hits(ctx.ui.ctx());
        hits.theme_cards.push((card.value.clone(), card_rect));
        set_hits(ctx.ui.ctx(), hits);
    }
}

/// 色板预览：12 列固定色块。
fn swatches_row(ctx: &mut RowCtx<'_>, colors: &[Color32]) {
    let panel = Rect::from_min_size(
        Pos2::new(ctx.x, ctx.y),
        Vec2::new(ctx.cursor.content_width(), ctx.h),
    );
    ctx.ui.painter().rect(
        panel,
        CornerRadius::same(21),
        ctx.palette.toolbar,
        Stroke::new(1.0, ctx.palette.input_border),
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
        ctx.ui.painter().rect(
            swatch,
            CornerRadius::same(3),
            *color,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 38)),
            egui::StrokeKind::Inside,
        );
    }
}

/// 关于页 Logo：可见时登记点击命中区。
fn about_logo_row(
    ctx: &mut RowCtx<'_>,
    dx: f32,
    size: f32,
    visible: bool,
) -> Option<(UiField, Rect)> {
    if !visible {
        return None;
    }
    let logo = Rect::from_min_size(Pos2::new(ctx.x + 7.0 + dx, ctx.y), Vec2::splat(size));
    ctx.icons.paint(ctx.ui.painter(), Icon::LogoInApp, logo);
    let response = ctx.ui.interact(
        logo.expand(7.0),
        egui::Id::unique("ui-about-logo"),
        Sense::click(),
    );
    if response.clicked() {
        action::push_action(
            ctx.ui.ctx(),
            SettingsAction::UiButton {
                field: UiField::AboutLogoClicked,
            },
        );
    }
    Some((UiField::AboutLogoClicked, logo))
}

/// 回声洞入口：整行可点。
fn echo_cave_row(ctx: &mut RowCtx<'_>, text: &str, opacity: f32) -> Option<(UiField, Rect)> {
    let row = Rect::from_min_size(
        Pos2::new(ctx.x, ctx.y),
        Vec2::new(ctx.cursor.content_width(), 18.0),
    );
    let color = Color32::from_rgba_unmultiplied(
        ctx.palette.text.r(),
        ctx.palette.text.g(),
        ctx.palette.text.b(),
        (255.0 * opacity.clamp(0.0, 1.0)).round() as u8,
    );
    ctx.ui.painter().text(
        Pos2::new(row.left(), row.center().y),
        Align2::LEFT_CENTER,
        crate::text::truncate_to_width(ctx.ui, text, 12.0, row.width()),
        FontId::proportional(12.0),
        color,
    );
    let response = ctx
        .ui
        .interact(row, egui::Id::unique("ui-echo-cave"), Sense::click());
    if response.clicked() {
        action::push_action(
            ctx.ui.ctx(),
            SettingsAction::UiButton {
                field: UiField::EchoCaveClicked,
            },
        );
    }
    Some((UiField::EchoCaveClicked, row))
}
