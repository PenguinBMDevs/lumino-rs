//! 对话框草稿：未保存更改确认。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Stroke, Vec2};

use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;
use crate::widgets;

/// 对话框草稿状态。
#[derive(Default)]
pub struct DialogState {
    /// 图标缓存（必须跨帧存活，否则纹理会在渲染前被释放）。
    pub icons: IconCache,
}

/// 渲染保存确认对话框。
pub fn show(ui: &mut egui::Ui, state: &mut DialogState) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

    egui::Panel::top("save-confirm-titlebar")
        .exact_size(layout::TITLEBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(crate::chrome::panel_frame(theme::TITLEBAR_BG))
        .show(ui, |ui| titlebar(ui, &mut state.icons));

    egui::CentralPanel::default()
        .frame(crate::chrome::panel_frame(theme::BASE_BG))
        .show(ui, content);
}

fn titlebar(ui: &mut egui::Ui, cache: &mut IconCache) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::TITLEBAR_BG);
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        widgets::traffic_button(ui, cache, Icon::WindowClose, true);
        widgets::traffic_button(ui, cache, Icon::WindowMax, false);
        widgets::traffic_button(ui, cache, Icon::WindowMin, false);
    });
}

fn content(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);

    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 90.0),
        Align2::CENTER_CENTER,
        "是否保留未保存的更改?",
        FontId::proportional(theme::font::TITLE),
        theme::TEXT,
    );

    let button_w = 90.0;
    let button_h = 35.0;
    let gap = 10.0;
    let total = button_w * 3.0 + gap * 2.0;
    let start_x = rect.center().x - total / 2.0;
    let y = rect.top() + 126.0;

    put_button(
        ui,
        Rect::from_min_size(egui::pos2(start_x, y), Vec2::new(button_w, button_h)),
        "保存",
        theme::ACCENT,
        Color32::from_rgb(0x12, 0x1A, 0x2A),
        theme::ACCENT_DARK,
    );
    put_button(
        ui,
        Rect::from_min_size(
            egui::pos2(start_x + button_w + gap, y),
            Vec2::new(button_w, button_h),
        ),
        "关闭",
        theme::TOOLBAR_BG,
        theme::TEXT,
        theme::ROUTE_BAR_BG,
    );
    put_button(
        ui,
        Rect::from_min_size(
            egui::pos2(start_x + (button_w + gap) * 2.0, y),
            Vec2::new(button_w, button_h),
        ),
        "取消",
        theme::TOOLBAR_BG,
        theme::TEXT,
        theme::ROUTE_BAR_BG,
    );
}

#[allow(clippy::too_many_arguments)]
fn put_button(
    ui: &mut egui::Ui,
    rect: Rect,
    text: &str,
    fill: Color32,
    text_color: Color32,
    border: Color32,
) {
    ui.put(
        rect,
        egui::Button::new(
            RichText::new(text)
                .size(theme::font::LABEL)
                .color(text_color),
        )
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .min_size(rect.size())
        .corner_radius(CornerRadius::same(4)),
    );
}
