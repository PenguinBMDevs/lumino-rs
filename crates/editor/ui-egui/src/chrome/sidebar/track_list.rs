//! 左侧音轨列表：编号 / 名称分列对齐（只读模型版）。
//!
//! 几何来自生产 iced `sidebar/panel.rs` + `sidebar/panel/track_item.rs`：
//! - 面板内容宽 = panel_width - 6（拖拽手柄），列内边距 8，右侧 8px 滚动条槽；
//! - 行按钮水平内边距 10，行内容内边距 4（添加行为 6）；
//! - 第一列固定 36 宽（含 2px 左内边距），列间距 4 → 名称列起点 = 行左 + 54
//!   （指挥轨图标同列，保证所有名称纵向对齐）；
//! - 独奏 `S` 右对齐到内容右边界（独奏时用警告色）；
//! - 音轨行高 35，添加行高 40。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, Sense, Vec2};

use super::super::{SidebarModel, SidebarTrack};
use super::panel_action::{self, SidebarPanelAction};
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;

/// 渲染音轨列表（标题 + 行 + 添加音轨）。
pub(super) fn show_model(
    ui: &mut egui::Ui,
    content: Rect,
    model: &SidebarModel,
    icons: &mut IconCache,
) {
    let list_left = content.left() + 8.0;
    // iced 的滚动条不占布局宽度（内容溢出时覆盖在右侧 8px 上），
    // 因此行宽 = 面板内容宽 - 左右各 8px 内边距。
    let list_right = content.right() - 8.0;
    let list_top = content.top() + layout::TRACK_LIST_HEADER_H;

    ui.painter().text(
        egui::pos2(list_left, content.top() + 15.0),
        Align2::LEFT_CENTER,
        &model.track_list_title,
        FontId::proportional(12.0),
        theme::text(),
    );

    // 内容底色只铺「音轨行 + 添加行」的高度（与 iced 的可滚动内容同高），
    // 面板其余区域仍透出面板底色——否则整块面板会比 iced 多一层底色。
    let content_height = layout::TRACK_ROW_H * model.tracks.len() as f32 + layout::TRACK_ADD_ROW_H;
    let background = Rect::from_min_size(
        egui::pos2(list_left, list_top),
        Vec2::new(list_right - list_left, content_height),
    );
    ui.painter().rect_filled(background, 0, theme::base_bg());

    // 手工逐行布局（不再用 `TableBuilder`）：表格的行间距会把「添加音轨」行
    // 推到最后一行的下半部分（实测行距 ≈ 41 vs 行高 36 → 10px 重叠）。
    // iced 的 `column![...rows, add]` 是 0 间距，这里保持一致。
    let list_area = Rect::from_min_max(
        egui::pos2(list_left, list_top),
        egui::pos2(list_right, content.bottom()),
    );
    let mut list_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(list_area)
            .layout(Layout::top_down(Align::Min)),
    );
    list_ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .id_salt("lumino-track-list")
        .auto_shrink([false, false])
        .show(&mut list_ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for track in &model.tracks {
                track_row(ui, track, icons);
            }
            let (add_rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), layout::TRACK_ADD_ROW_H),
                Sense::hover(),
            );
            add_track_row(ui, add_rect, &model.add_track_label, icons);
        });
}

/// 仅「添加音轨」行（`Route::Arrangement`）：面板内边距 8，行宽 = 内容宽 - 16。
pub(super) fn show_add_track(
    ui: &mut egui::Ui,
    content: Rect,
    model: &SidebarModel,
    icons: &mut IconCache,
) {
    let rect = Rect::from_min_size(
        egui::pos2(content.left() + 8.0, content.top() + 8.0),
        Vec2::new((content.width() - 16.0).max(0.0), layout::TRACK_ADD_ROW_H),
    );
    add_track_row(ui, rect, &model.add_track_label, icons);
}

/// 单个音轨行：编号列（固定 36）+ 名称列（固定起点）+ 右侧独奏。
///
/// 行由 egui **独占**点击：命中即 `SelectTrack(id)` 并计入占用区，避免 iced
/// 的同位行按钮再处理一次（两侧选择同一个音轨时不会报错，但会造成两次重绘）。
fn track_row(ui: &mut egui::Ui, track: &SidebarTrack, cache: &mut IconCache) {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), layout::TRACK_ROW_H),
        Sense::click(),
    );
    if response.clicked() {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::SelectTrack(track.id));
    }
    // 行命中区由 egui 独占（iced 的同位行按钮不再处理同一次点击）。
    super::super::menu::extend_blocked(ui.ctx(), [rect]);

    let text_color = track_text_color(track);
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
        cache.paint(ui.painter(), Icon::Clock, icon_rect);
    } else {
        ui.painter().text(
            egui::pos2(column_left, rect.center().y),
            Align2::LEFT_CENTER,
            &track.label,
            FontId::proportional(theme::font::LABEL),
            text_color,
        );
    }

    // 第二列：名称。指挥轨图标列 24 宽（生产 `track_item` 的图标容器），
    // 普通轨编号列 36 宽，因此指挥轨名称起点比其它轨早 12px（与 iced 一致）。
    let label_col = if track.conductor {
        layout::TRACK_ADD_ICON_COL_W
    } else {
        layout::TRACK_LABEL_COL_W
    };
    let name_left = inner.left() + label_col + layout::TRACK_COL_GAP;
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
        // egui 默认 `selectable_labels = true`：名称标签会自己吃掉指针事件，
        // 表现为「点音轨名选不中音轨」（只有点行内空白才有效）。这里显式关闭。
        .selectable(false)
        .truncate(),
    );

    // 独奏按钮右对齐到内容右边界（独奏时警告色）。
    ui.painter().text(
        egui::pos2(inner.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        "S",
        FontId::proportional(theme::font::LABEL),
        if track.soloed {
            theme::warning()
        } else {
            text_color
        },
    );
}

/// 「添加音轨」行：图标列 24 + 列间距 4。
fn add_track_row(ui: &mut egui::Ui, rect: Rect, label: &str, cache: &mut IconCache) {
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), theme::base_bg());
    // 命中区与绘制同源；点击只给 egui（否则 iced 的添加按钮会再加一条音轨）。
    let response = ui.interact(
        rect,
        egui::Id::unique(("lumino.add-track", rect.top() as i32)),
        Sense::click(),
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::toolbar_bg());
    }
    if response.clicked() {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::AddTrack);
    }
    super::super::menu::extend_blocked(ui.ctx(), [rect]);

    let inner_left = rect.left() + layout::TRACK_BUTTON_PAD_X + layout::TRACK_ADD_ROW_PAD;
    let icon_rect = Rect::from_min_size(
        egui::pos2(inner_left + 2.0, rect.center().y - 9.0),
        Vec2::splat(18.0),
    );
    cache.paint(ui.painter(), Icon::Plus, icon_rect);

    ui.painter().text(
        egui::pos2(
            inner_left + layout::TRACK_ADD_ICON_COL_W + layout::TRACK_COL_GAP,
            rect.center().y,
        ),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(theme::font::LABEL),
        theme::text(),
    );
}

/// 音轨文本色：自定义色按亮度取黑/白，默认主题正文色。
fn track_text_color(track: &SidebarTrack) -> Color32 {
    match track.color {
        Some(color) => {
            let luminance = 0.299 * f32::from(color.r())
                + 0.587 * f32::from(color.g())
                + 0.114 * f32::from(color.b());
            if luminance > 127.5 {
                Color32::BLACK
            } else {
                Color32::WHITE
            }
        }
        None => theme::text(),
    }
}

/// 行底色：与 iced `track_button_background` 同规则。
fn row_background(track: &SidebarTrack, hovered: bool) -> Color32 {
    match (track.color, track.selected, hovered) {
        (Some(color), true, _) => blend(color, theme::selected_bg(), 0.35),
        (Some(color), false, true) => blend(color, theme::toolbar_bg(), 0.25),
        (Some(color), false, false) => color,
        (None, true, _) => theme::selected_bg(),
        (None, false, true) => theme::toolbar_bg(),
        (None, false, false) => theme::base_bg(),
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
