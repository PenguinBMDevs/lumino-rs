//! 左侧音轨列表：编号 / 名称分列对齐。
//!
//! 几何来自生产 iced `sidebar/panel.rs` + `sidebar/panel/track_item.rs`：
//! - 面板内容宽 194（200 - 6px 拖拽手柄），列内边距 8，右侧 8px 滚动条槽；
//! - 行按钮水平内边距 10，行内容内边距 4（添加行为 6）；
//! - 第一列固定 36 宽（含 2px 左内边距），列间距 4 → 名称列起点 = 行左 + 54；
//! - 独奏 `S` 右对齐到内容右边界；
//! - 音轨行高 35（14px 文本行高 17 + 内边距 8 + 按钮内边距 10），
//!   添加行高 40（图标 18 + 内边距 12 + 按钮内边距 10）。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, Sense, Vec2};
use egui_extras::{Column, TableBuilder};

use super::super::{ChromeState, TrackRow};
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;

/// 渲染音轨列表（标题 + 行 + 添加音轨）。
pub(super) fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    let panel = ui.max_rect();
    // 生产 iced：`row![content(194), resize_handle(6)]`，滚动条 8 占内容右侧。
    let content_right = panel.left() + layout::SIDEBAR_PANEL_W - layout::SIDEBAR_RESIZE_W;
    let list_left = panel.left() + 8.0;
    let list_right = content_right - layout::SIDEBAR_SCROLLBAR_W;
    let list_top = panel.top() + layout::TRACK_LIST_HEADER_H;

    let tracks = state.tracks.clone();
    let icons = &mut state.icons;
    let mut selected = tracks.iter().position(|track| track.selected).unwrap_or(0);

    ui.painter().text(
        egui::pos2(list_left, panel.top() + 15.0),
        Align2::LEFT_CENTER,
        "音轨列表",
        FontId::proportional(12.0),
        theme::TEXT,
    );

    let rows_height = layout::TRACK_ROW_H * tracks.len() as f32;
    let list_rect = Rect::from_min_size(
        egui::pos2(list_left, list_top),
        Vec2::new(
            list_right - list_left,
            rows_height + layout::TRACK_ADD_ROW_H,
        ),
    );
    ui.painter().rect_filled(list_rect, 0, theme::BASE_BG);

    let mut list_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                list_rect.min,
                Vec2::new(list_rect.width(), rows_height),
            ))
            .layout(Layout::top_down(Align::Min)),
    );

    TableBuilder::new(&mut list_ui)
        .id_salt("lumino-track-list")
        .striped(false)
        .column(Column::remainder())
        .body(|body| {
            body.rows(layout::TRACK_ROW_H, tracks.len(), |mut row| {
                let index = row.index();
                row.col(|ui| {
                    if track_row(ui, &tracks[index], icons).clicked() {
                        selected = index;
                    }
                });
            });
        });

    for (index, track) in state.tracks.iter_mut().enumerate() {
        track.selected = index == selected;
    }

    let add_rect = Rect::from_min_size(
        egui::pos2(list_left, list_top + rows_height),
        Vec2::new(list_rect.width(), layout::TRACK_ADD_ROW_H),
    );
    let mut add_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(add_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    add_track_row(&mut add_ui, &mut state.icons);
}

/// 单个音轨行：编号列（固定 36）+ 名称列（固定起点）+ 右侧独奏。
fn track_row(ui: &mut egui::Ui, track: &TrackRow, cache: &mut IconCache) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), layout::TRACK_ROW_H),
        Sense::click(),
    );

    let text_color = track.color.unwrap_or(theme::TEXT);
    let background = row_background(track, response.hovered());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), background);

    // 行按钮内边距 10 + 行内容内边距 4。
    let inset = layout::TRACK_BUTTON_PAD_X + layout::TRACK_ROW_PAD;
    let inner = Rect::from_min_max(
        egui::pos2(rect.left() + inset, rect.top()),
        egui::pos2(rect.right() - inset, rect.bottom()),
    );

    // 第一列：编号 / 指挥轨图标，固定 36 宽，左内边距 2。
    let column_left = inner.left() + 2.0;
    if track.conductor {
        let icon_rect = Rect::from_center_size(
            egui::pos2(column_left + 9.0, rect.center().y),
            Vec2::splat(18.0),
        );
        ui.painter().image(
            cache
                .texture(ui.ctx(), Icon::Clock, 18.0, Icon::Clock.default_tint())
                .id(),
            icon_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().text(
            egui::pos2(column_left, rect.center().y),
            Align2::LEFT_CENTER,
            &track.label,
            FontId::proportional(theme::font::LABEL),
            text_color,
        );
    }

    // 第二列：名称，起点固定，长度截断到独奏按钮之前。
    let name_left = inner.left() + layout::TRACK_LABEL_COL_W + layout::TRACK_COL_GAP;
    let name_rect = Rect::from_min_max(
        egui::pos2(name_left, rect.top()),
        egui::pos2(inner.right() - 12.0, rect.bottom()),
    );
    let mut name_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(name_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    name_ui.add(
        egui::Label::new(
            egui::RichText::new(&track.name)
                .size(theme::font::LABEL)
                .color(text_color),
        )
        .truncate(),
    );

    // 独奏按钮右对齐到内容右边界。
    ui.painter().text(
        egui::pos2(inner.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        "S",
        FontId::proportional(theme::font::LABEL),
        text_color,
    );

    response
}

/// 「添加音轨」行：图标列 24 + 列间距 4。
fn add_track_row(ui: &mut egui::Ui, cache: &mut IconCache) {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), layout::TRACK_ADD_ROW_H),
        Sense::click(),
    );
    let background = if response.hovered() {
        theme::TOOLBAR_BG
    } else {
        theme::BASE_BG
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), background);

    let inner_left = rect.left() + layout::TRACK_BUTTON_PAD_X + layout::TRACK_ADD_ROW_PAD;
    let icon_rect = Rect::from_min_size(
        egui::pos2(inner_left + 2.0, rect.center().y - 9.0),
        Vec2::splat(18.0),
    );
    ui.painter().image(
        cache
            .texture(ui.ctx(), Icon::Plus, 18.0, Icon::Plus.default_tint())
            .id(),
        icon_rect,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    ui.painter().text(
        egui::pos2(
            inner_left + layout::TRACK_ADD_ICON_COL_W + layout::TRACK_COL_GAP,
            rect.center().y,
        ),
        Align2::LEFT_CENTER,
        "添加音轨",
        FontId::proportional(theme::font::LABEL),
        theme::TEXT,
    );
}

/// 行底色：与 iced `track_button_background` 同规则。
fn row_background(track: &TrackRow, hovered: bool) -> Color32 {
    match (track.color, track.selected, hovered) {
        (Some(color), true, _) => blend(color, theme::SELECTED_BG, 0.35),
        (Some(color), false, true) => blend(color, theme::TOOLBAR_BG, 0.25),
        (Some(color), false, false) => color,
        (None, true, _) => theme::SELECTED_BG,
        (None, false, true) => theme::TOOLBAR_BG,
        (None, false, false) => theme::BASE_BG,
    }
}

/// 线性混合两个不透明颜色（`t` = 目标色占比）。
fn blend(from: Color32, to: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}
