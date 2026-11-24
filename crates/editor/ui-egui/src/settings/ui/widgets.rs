//! 界面页通用控件：主题卡片、截断标签、开关行、行高计算。

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::super::chrome::{SettingsPalette, ThemeCardModel, UiField, UiRow};
use super::super::pages::switch_style;
use super::{BAR_H, CARD_GAP, CARD_H, CARD_W, MOCK_PAD};
use crate::widgets::text_width;

/// 主题卡片：底色 / 标题 / 迷你面板 / 选中描边。
pub(super) fn draw_theme_card(ui: &mut egui::Ui, rect: Rect, card: &ThemeCardModel) {
    let response = ui.interact(
        rect,
        egui::Id::unique(("ui-theme-card-paint", card.value.as_str())),
        Sense::hover(),
    );
    let border = if card.selected {
        (2.0, card.accent)
    } else if response.hovered() {
        (1.5, card.line)
    } else {
        (
            1.0,
            Color32::from_rgba_unmultiplied(card.line.r(), card.line.g(), card.line.b(), 115),
        )
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        card.bg,
        Stroke::new(border.0, border.1),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        Pos2::new(rect.left() + 8.0, rect.top() + 8.0),
        Align2::LEFT_TOP,
        crate::text::truncate_to_width(ui, &card.display, 11.0, rect.width() - 16.0),
        FontId::proportional(11.0),
        card.title,
    );
    // 迷你面板：控件底 + 主/次文字条 + 强调色条 + 两级分隔线。
    let panel = Rect::from_min_size(
        Pos2::new(rect.left() + 8.0, rect.top() + 30.0),
        Vec2::new(rect.width() - 16.0, rect.height() - 38.0),
    );
    ui.painter()
        .rect_filled(panel, CornerRadius::same(4), card.control);
    let inner = panel.shrink(MOCK_PAD);
    let mut y = inner.top();
    for (color, width) in [
        (card.bar_primary, 36.0),
        (card.bar_secondary, 62.0),
        (card.accent, 20.0),
    ] {
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(width, BAR_H)),
            CornerRadius::same(2),
            color,
        );
        y += BAR_H + 4.0;
    }
    y += 2.0;
    ui.painter().rect_filled(
        Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 1.0)),
        0,
        card.line,
    );
    y += 2.0;
    ui.painter().rect_filled(
        Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 1.0)),
        0,
        card.line_soft,
    );
}

pub(super) fn draw_label(
    ui: &mut egui::Ui,
    x: f32,
    center_y: f32,
    label: &str,
    hint: &str,
    max_width: f32,
    palette: &SettingsPalette,
) {
    let label = crate::widgets::truncate_to_width(ui, label, 14.0, max_width.max(1.0));
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
    ui.interact(
        rect,
        egui::Id::unique(("ui-label-hint", label.as_str())),
        Sense::hover(),
    )
    .on_hover_text(hint);
}

/// 主题卡片网格列数（按卡片宽度自适应）。
pub(super) fn grid_columns(width: f32) -> usize {
    let usable = (width - super::engine::PAD * 2.0).max(CARD_W);
    (((usable + CARD_GAP) / (CARD_W + CARD_GAP)).floor() as usize).max(1)
}

/// 一行的高度（开关 / 下拉 / 滑杆 / 文本 / 卡片网格…）。
///
/// `Static` 是说明性文本：宽度不够时折行，高度必须随行数增长（不能硬画单行）。
pub(super) fn row_height(ui: &egui::Ui, row: &UiRow, width: f32) -> f32 {
    match row {
        UiRow::Section { .. } => 26.0,
        UiRow::Combo { .. } => 32.0,
        UiRow::Text { .. } => 31.0,
        UiRow::Checkbox { .. } => 22.0,
        UiRow::Slider { .. } => 22.0,
        UiRow::Button { .. } => 30.0,
        UiRow::Static { text } => {
            // 页面左右各 20 内边距；12px 字号按整行宽度折行。
            let wrap = (width - 40.0).max(1.0);
            crate::text::wrapped_height(ui, text, 12.0, wrap).max(18.0)
        }
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

/// 一行开关：绘制 + 返回命中结果（标签在左、开关贴行右缘）。
pub(super) fn switch_row(
    ui: &mut egui::Ui,
    row: Rect,
    field: UiField,
    label: &str,
    hint: &str,
    checked: bool,
    palette: &SettingsPalette,
) -> crate::widgets::SwitchHit {
    crate::widgets::switch_row(
        ui,
        row,
        &crate::widgets::SwitchRow {
            id: egui::Id::unique(("ui-switch", field as u8)),
            checked,
            enabled: true,
            label,
            hint,
        },
        &switch_style(palette),
    )
}
