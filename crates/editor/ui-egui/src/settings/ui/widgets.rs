//! 界面页通用控件：主题卡片、截断标签、对钩框。

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::super::chrome::{SettingsPalette, ThemeCardModel, UiField};
use super::super::action::{self, SettingsAction};
use super::super::pages::text_width;
use super::{BAR_H, MOCK_PAD};

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
        &card.display,
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
    ui.interact(
        rect,
        egui::Id::unique(("ui-label-hint", label.as_str())),
        Sense::hover(),
    )
    .on_hover_text(hint);
}

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

#[allow(clippy::too_many_arguments)]
pub(super) fn checkbox_row(
    ui: &mut egui::Ui,
    x: f32,
    y: f32,
    checked: bool,
    label: &str,
    hint: &str,
    field: UiField,
    palette: &SettingsPalette,
) -> Rect {
    let box_rect = Rect::from_min_size(Pos2::new(x, y + 2.0), Vec2::splat(18.0));
    let label_w = text_width(ui, label, 14.0);
    let label_rect = Rect::from_min_size(
        Pos2::new(box_rect.right() + 8.0, y - 1.0),
        Vec2::new(label_w, 24.0),
    );
    let mut toggled = false;
    if ui
        .interact(
            box_rect,
            egui::Id::unique(("ui-check", field as u8)),
            Sense::click(),
        )
        .clicked()
    {
        toggled = true;
    }
    let label_response = ui.interact(
        label_rect,
        egui::Id::unique(("ui-check-label", field as u8)),
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
            SettingsAction::UiToggle {
                field,
                checked: !checked,
            },
        );
    }
    box_rect
}
