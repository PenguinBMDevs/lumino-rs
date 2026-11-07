//! 自定义标题栏：菜单、拖动区、窗口控制按钮。

use egui::{Align, Layout, Vec2};
use egui_extras::{Size, StripBuilder};

use super::ChromeState;
use crate::icons::Icon;
use crate::layout;
use crate::theme;
use crate::widgets;

const MENU_ITEM_W: f32 = 48.0;
const LOGO_W: f32 = 48.0;
const MENU_TOTAL_W: f32 = LOGO_W + MENU_ITEM_W * 4.0;
const TRAFFIC_TOTAL_W: f32 = layout::TRAFFIC_BTN_W * 3.0;

/// 渲染标题栏。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::TITLEBAR_BG);

    StripBuilder::new(ui)
        .size(Size::exact(MENU_TOTAL_W))
        .size(Size::remainder())
        .size(Size::exact(TRAFFIC_TOTAL_W))
        .horizontal(|mut strip| {
            strip.cell(|ui| menu(ui, state));
            strip.cell(|ui| {
                ui.allocate_space(ui.available_size());
            });
            strip.cell(|ui| traffic(ui, state));
        });
}

fn menu(ui: &mut egui::Ui, state: &mut ChromeState) {
    ui.horizontal_centered(|ui| {
        ui.add_space(15.0);
        ui.add(state.icons.default_image(ui.ctx(), Icon::LogoInApp, 18.0));
        ui.add_space(15.0);
        for item in ["文件", "编辑", "视图", "帮助"] {
            ui.allocate_ui_with_layout(
                Vec2::new(MENU_ITEM_W, layout::TITLEBAR_H),
                Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    widgets::menu_label(ui, item, false);
                },
            );
        }
    });
}

fn traffic(ui: &mut egui::Ui, state: &mut ChromeState) {
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        widgets::traffic_button(ui, &mut state.icons, Icon::WindowClose, true);
        widgets::traffic_button(ui, &mut state.icons, Icon::WindowMax, false);
        widgets::traffic_button(ui, &mut state.icons, Icon::WindowMin, false);
    });
}
