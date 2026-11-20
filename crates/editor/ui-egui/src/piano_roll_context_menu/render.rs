//! 钢琴卷帘右键菜单的绘制（单竖列：分隔线 / 行 / 聚焦框 / 滚动条）。

use egui::{Color32, CornerRadius, FontId, Rect, Stroke, StrokeKind, Vec2};
use lumino_message::context_menu::{item_disabled_hint, item_enabled, item_label};
use lumino_message::{PianoRollContextMenuAction, PianoRollContextMenuItem as Item};

use super::icons::item_icon;
use super::{
    FOCUS_RING, ICON, ICON_DISABLED, ICON_GAP, MenuFocus, MenuLayout, PANEL_BG, PianoRollMenuModel,
    ROW_HOVER, ROW_PAD, ROW_PRESSED, SCROLL_BAR_GAP, SCROLL_BAR_W, SCROLL_THUMB, SCROLL_TRACK,
    SEPARATOR, SEPARATOR_H, TEXT, TEXT_DISABLED, TEXT_SIZE,
};
use crate::icons::IconCache;

/// 绘制整块面板；返回本帧被点击的条目。
pub(super) fn panel(
    ui: &egui::Ui,
    model: &PianoRollMenuModel,
    items: &[Item],
    layout: &MenuLayout,
    focus: &MenuFocus,
    icons: &mut IconCache,
) -> Option<PianoRollContextMenuAction> {
    let painter = ui.painter().with_clip_rect(model.available);
    painter.rect(
        layout.panel,
        CornerRadius::same(8),
        PANEL_BG,
        Stroke::new(0.0, Color32::TRANSPARENT),
        StrokeKind::Inside,
    );

    draw_separators(ui, layout);
    let clicked = draw_rows(ui, model, items, layout, icons);
    draw_focus(ui, layout, focus);
    draw_scrollbar(ui, layout);
    clicked
}

/// 组间分隔线（随滚动一起移动）。
fn draw_separators(ui: &egui::Ui, layout: &MenuLayout) {
    let painter = ui.painter().with_clip_rect(layout.content);
    for group in layout.groups.iter().skip(1) {
        let y = layout.content.top() + group.top - layout.scroll - SEPARATOR_H * 0.5;
        if y < layout.content.top() || y > layout.content.bottom() {
            continue;
        }
        painter.line_segment(
            [
                egui::pos2(layout.content.left() + 2.0, y),
                egui::pos2(layout.content.right() - 2.0, y),
            ],
            Stroke::new(1.0, SEPARATOR),
        );
    }
}

/// 逐行绘制图标 + 文字；只对落在视口内的部分注册命中。
fn draw_rows(
    ui: &egui::Ui,
    model: &PianoRollMenuModel,
    items: &[Item],
    layout: &MenuLayout,
    icons: &mut IconCache,
) -> Option<PianoRollContextMenuAction> {
    let mut clicked = None;
    for (row, item) in items.iter().copied().enumerate() {
        let rect = super::item_rect(layout, row);
        let visible = rect.intersect(layout.content);
        if visible.width() <= 0.0 || visible.height() <= 0.0 {
            continue;
        }
        let enabled = item_enabled(model.target, item, model.availability);
        let response = ui.interact(
            visible,
            egui::Id::unique(("lumino.piano-roll-menu.row", row)),
            egui::Sense::click(),
        );
        let pressed = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && response.hovered();
        let painter = ui
            .painter()
            .with_clip_rect(layout.content)
            .with_clip_rect(model.available);
        if pressed {
            painter.rect_filled(rect, CornerRadius::same(4), ROW_PRESSED);
        } else if hovered {
            painter.rect_filled(rect, CornerRadius::same(4), ROW_HOVER);
        }

        let icon_rect = Rect::from_center_size(
            egui::pos2(rect.left() + ROW_PAD + ICON * 0.5, rect.center().y),
            Vec2::splat(ICON),
        );
        icons.paint_tinted(
            &painter,
            item_icon(item),
            icon_rect,
            Some(if enabled { TEXT } else { ICON_DISABLED }),
        );
        let galley = painter.layout_no_wrap(
            item_label(item).to_owned(),
            FontId::proportional(TEXT_SIZE),
            if enabled { TEXT } else { TEXT_DISABLED },
        );
        painter.galley(
            egui::pos2(
                icon_rect.right() + ICON_GAP,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            Color32::WHITE,
        );

        let response = response.on_hover_cursor(if enabled {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::NotAllowed
        });
        // 文字已经可见，只有置灰项需要额外解释为什么点不了。
        let response = if enabled {
            response
        } else {
            response.on_hover_ui(|ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_premultiplied(20, 20, 26, 245))
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .corner_radius(CornerRadius::same(4))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(item_disabled_hint(
                                model.target,
                                item,
                                model.availability,
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(150, 150, 158)),
                        );
                    });
            })
        };
        if enabled && response.clicked() {
            clicked = Some(PianoRollContextMenuAction::ItemClicked(item));
        }
    }
    clicked
}

/// 键盘聚焦框（只在用过方向键且焦点行完整可见时出现）。
fn draw_focus(ui: &egui::Ui, layout: &MenuLayout, focus: &MenuFocus) {
    if !focus.keyboard {
        return;
    }
    let rect = super::item_rect(layout, focus.row);
    if rect == Rect::NOTHING || !layout.content.contains_rect(rect) {
        return;
    }
    ui.painter().with_clip_rect(layout.content).rect_stroke(
        rect.shrink(1.0),
        CornerRadius::same(4),
        Stroke::new(1.5, FOCUS_RING),
        StrokeKind::Inside,
    );
}

/// 内容高于视口时在右缘画细滚动条。
fn draw_scrollbar(ui: &egui::Ui, layout: &MenuLayout) {
    let max = layout.max_scroll();
    if max <= 0.0 {
        return;
    }
    let right = layout.panel.right() - SCROLL_BAR_GAP;
    let track = Rect::from_min_max(
        egui::pos2(right - SCROLL_BAR_W, layout.content.top()),
        egui::pos2(right, layout.content.bottom()),
    );
    let painter = ui.painter().with_clip_rect(layout.panel);
    painter.rect_filled(track, CornerRadius::same(2), SCROLL_TRACK);

    let ratio = layout.content.height() / layout.content_height;
    let thumb_h = (track.height() * ratio).max(24.0).min(track.height());
    let travel = (track.height() - thumb_h).max(0.0);
    let top = track.top() + travel * (layout.scroll / max);
    let thumb = Rect::from_min_size(
        egui::pos2(track.left(), top),
        Vec2::new(track.width(), thumb_h),
    );
    painter.rect_filled(thumb, CornerRadius::same(2), SCROLL_THUMB);
}
