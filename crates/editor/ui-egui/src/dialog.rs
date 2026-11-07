//! 对话框：未保存更改确认（整窗）。
//!
//! 几何来自生产 iced `view/save_confirm_dialog.rs`：
//! - 自制标题栏 30px（系统原生标题栏时跳过）；
//! - 提示语 16px、垂直居中于内容区顶部 90px；
//! - 三个按钮 90×35、间距 10，水平居中于内容区，顶边 = 内容区 + 126。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Stroke, Vec2};

use crate::chrome::SaveConfirmModel;
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;
use crate::widgets;

/// 对话框草稿状态（场景入口用）。
#[derive(Default)]
pub struct DialogState {
    /// 图标缓存（必须跨帧存活，否则纹理会在渲染前被释放）。
    pub icons: IconCache,
}

/// 场景入口：与生产同文案的保存确认对话框。
pub fn show(ui: &mut egui::Ui, state: &mut DialogState) {
    let model = SaveConfirmModel {
        titlebar: true,
        message: "是否保留未保存的更改?".to_owned(),
        save: "保存".to_owned(),
        close: "关闭".to_owned(),
        cancel: "取消".to_owned(),
    };
    show_model(ui, &model, &mut state.icons);
}

/// 按只读模型渲染整窗（真实窗口叠加层与场景共用）。
pub fn show_model(ui: &mut egui::Ui, model: &SaveConfirmModel, icons: &mut IconCache) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

    // `egui_kittest`（以及部分宿主）给到的 Ui 带 8px 外缩进；Panel 需要铺满视口。
    let full = ui.ctx().viewport_rect();
    let mut root = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(full)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    root.set_clip_rect(full);
    root.spacing_mut().item_spacing = egui::Vec2::ZERO;

    if model.titlebar {
        egui::Panel::top("save-confirm-titlebar")
            .exact_size(layout::TITLEBAR_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(crate::chrome::panel_frame(theme::TITLEBAR_BG))
            .show(&mut root, |ui| titlebar(ui, icons));
    }

    egui::CentralPanel::default()
        .frame(crate::chrome::panel_frame(theme::BASE_BG))
        .show(&mut root, |ui| content(ui, model));
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

fn content(ui: &mut egui::Ui, model: &SaveConfirmModel) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::BASE_BG);

    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 90.0),
        Align2::CENTER_CENTER,
        &model.message,
        // 生产 iced：提示语 16px（	ext(...).size(16)）。
        FontId::proportional(theme::font::SECTION),
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
        &model.save,
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
        &model.close,
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
        &model.cancel,
        theme::TOOLBAR_BG,
        theme::TEXT,
        theme::ROUTE_BAR_BG,
    );
}

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
