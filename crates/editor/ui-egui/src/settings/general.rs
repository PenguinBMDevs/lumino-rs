//! 设置「常规」页：橡皮擦 / 加轨行为 / 日志保留。

use egui::{Layout, Rect, Vec2};
use lumino_core::storage::config::{EraserBehavior, TrackAddBehavior};

use super::super::chrome::SettingsPalette;
use super::super::{layout, theme, widgets};
use super::SettingsState;
use super::action::{self, SettingsAction};

/// 渲染常规页表单（几何与 iced 基线逐像素对齐）。
pub(super) fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &mut SettingsState,
    palette: &SettingsPalette,
) {
    let pad = layout::SETTINGS_PADDING;
    let left = rect.left() + pad;
    let base = rect.top();

    // 文案副本：下方控件需要可变借用 `state`（输入框）。
    let title = state.labels.title.clone();
    let eraser_label = state.labels.eraser_label.clone();
    let eraser_options = state.labels.eraser_options.clone();
    let track_add_label = state.labels.track_add_label.clone();
    let track_add_options = state.labels.track_add_options.clone();
    let log_section = state.labels.log_section.clone();
    let log_label = state.labels.log_label.clone();

    ui.painter().text(
        egui::pos2(left, base + 30.0),
        egui::Align2::LEFT_CENTER,
        &title,
        egui::FontId::proportional(theme::font::TITLE),
        palette.text,
    );

    // 表单：标签 + 间距 32（8+16+8，与生产 `row![label, space(16), pick_list]` 一致），
    // 控件起点随标签实际宽度变化——英文标签更长，固定列宽会压住下拉框。
    let label_x = left;
    if let Some(index) = form_row(
        ui,
        RowGeometry {
            label_x,
            center_y: base + 99.0,
        },
        &eraser_label,
        &state.eraser,
        &eraser_options,
        palette,
    ) {
        let behavior = match index {
            0 => EraserBehavior::Default,
            _ => EraserBehavior::DirectSelect,
        };
        action::push_action(ui.ctx(), SettingsAction::EraserBehavior(behavior));
    }
    if let Some(index) = form_row(
        ui,
        RowGeometry {
            label_x,
            center_y: base + 171.0,
        },
        &track_add_label,
        &state.track_add,
        &track_add_options,
        palette,
    ) {
        let behavior = match index {
            0 => TrackAddBehavior::AutoSwitch,
            _ => TrackAddBehavior::StayCurrent,
        };
        action::push_action(ui.ctx(), SettingsAction::TrackAddBehavior(behavior));
    }

    ui.painter().text(
        egui::pos2(left, base + 236.0),
        egui::Align2::LEFT_CENTER,
        &log_section,
        egui::FontId::proportional(theme::font::SECTION),
        palette.text,
    );

    let log_label_width = ui
        .painter()
        .layout_no_wrap(
            log_label.clone(),
            egui::FontId::proportional(theme::font::LABEL),
            palette.text,
        )
        .size()
        .x;
    ui.painter().text(
        egui::pos2(label_x, base + 291.0),
        egui::Align2::LEFT_CENTER,
        &log_label,
        egui::FontId::proportional(theme::font::LABEL),
        palette.text,
    );
    // 生产 iced 的行布局：`row![label, space(16), input]`，行间距 8 →
    // 标签与输入框之间实际 32px（8+16+8）。
    let input_rect = Rect::from_min_size(
        egui::pos2(
            label_x + log_label_width + 32.0,
            base + 291.0 - layout::SETTINGS_INPUT_H / 2.0,
        ),
        Vec2::new(80.0, layout::SETTINGS_INPUT_H),
    );
    let mut input_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(input_rect)
            .layout(Layout::top_down(egui::Align::Min)),
    );
    let response = widgets::numeric_input(&mut input_ui, &mut state.log_retention, 80.0, palette);
    if response.changed() {
        // 原样回传（生产统一 parse + clamp，非法输入不生效）。
        action::push_action(
            ui.ctx(),
            SettingsAction::LogRetention(state.log_retention.clone()),
        );
    }
}

/// 表单行几何（标签列起点 / 行中心）。
#[derive(Debug, Clone, Copy)]
struct RowGeometry {
    label_x: f32,
    center_y: f32,
}

/// 标签 + 下拉框；返回用户选中的选项下标。
fn form_row(
    ui: &mut egui::Ui,
    geometry: RowGeometry,
    label: &str,
    value: &str,
    options: &[String],
    palette: &SettingsPalette,
) -> Option<usize> {
    let label_font = egui::FontId::proportional(theme::font::LABEL);
    let label_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), label_font.clone(), palette.text)
        .size()
        .x;
    ui.painter().text(
        egui::pos2(geometry.label_x, geometry.center_y),
        egui::Align2::LEFT_CENTER,
        label,
        label_font,
        palette.text,
    );

    // 生产：`row![label, space(16), pick_list].spacing(8)` → 标签与控件间 8+16+8 = 32。
    let field_x = geometry.label_x + label_width + 32.0;
    let combo_rect = Rect::from_min_size(
        egui::pos2(field_x, geometry.center_y - 16.0),
        Vec2::new(200.0, 32.0),
    );
    let mut combo_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(combo_rect)
            .layout(Layout::top_down(egui::Align::Min)),
    );
    widgets::combo_box(&mut combo_ui, label, value, options, 200.0, palette)
}
