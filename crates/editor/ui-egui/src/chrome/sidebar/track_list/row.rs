//! 左侧栏音轨行：编号 / 名称 / 独奏 `S` 的绘制与命中。
//!
//! 从 `track_list.rs` 拆出（文件长度红线 < 400 行）。几何常量仍来自
//! `crate::layout`，与列表容器（标题 / 添加行）保持同一套口径。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, Sense, Vec2};

use super::super::super::SidebarTrack;
use super::super::panel_action::{self, SidebarPanelAction};
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;

/// 独奏按钮命中边长（像素）：与可视化 `S` 同源，保证「看到的就是点得到的」。
const SOLO_HIT: f32 = 18.0;

/// 单个音轨行：编号列（固定 36）+ 名称列（固定起点）+ 右侧独奏。
///
/// 行由 egui **独占**点击：命中即 `SelectTrack(id)` 并计入占用区，避免 上一代
/// 的同位行按钮再处理一次（两侧选择同一个音轨时不会报错，但会造成两次重绘）。
pub(super) fn track_row(
    ui: &mut egui::Ui,
    track: &SidebarTrack,
    cache: &mut IconCache,
    solo_tip: crate::widgets::HoverTip,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), layout::TRACK_ROW_H),
        Sense::click_and_drag(),
    );

    // 行按钮内边距 10 + 行内容内边距 4。
    let inset = layout::TRACK_BUTTON_PAD_X + layout::TRACK_ROW_PAD;
    let inner = Rect::from_min_max(
        egui::pos2(rect.left() + inset, rect.top()),
        egui::pos2(rect.right() - inset, rect.bottom()),
    );

    // 独奏按钮：命中区与绘制同源（右对齐到内容右边界）。
    // 上一代 `track_item` 的 S 是 `button("S").on_press(track_solo_toggled)`；
    // 迁移到 egui 后只画了文字没接命中，点击会落到整行选择上（真机表现：S 点不动）。
    let solo_rect = Rect::from_center_size(
        egui::pos2(inner.right() - SOLO_HIT * 0.5, rect.center().y),
        Vec2::splat(SOLO_HIT),
    );
    let click_in_solo = response
        .interact_pointer_pos()
        .is_some_and(|pos| solo_rect.contains(pos));
    // 兜底：即使命中判定把点击给了整行，落在 S 上时也只切独奏。
    if response.clicked() && click_in_solo {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::TrackSoloToggled(track.id));
    }
    // 点 S 只切独奏，不同时改选中的音轨；行点击保持原语义。
    if response.clicked() && !click_in_solo {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::SelectTrack(track.id));
    }
    if response.secondary_clicked() {
        // 右键由 egui 接管（原同位行按钮已不存在）。
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::OpenTrackMenu(track.id));
    }
    // 行命中区由 egui 独占。
    // 例外：行内重命名时整行不占用，保证重命名输入框的指针操作不被拦掉。
    if !track.renaming {
        super::super::super::menu::extend_blocked(ui.ctx(), [rect]);
    }

    let text_color = track_text_color(track);
    let background = row_background(track, response.hovered());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), background);

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
    // 普通轨编号列 36 宽，因此指挥轨名称起点比其它轨早 12px（与 上一代 一致）。
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
    if track.renaming {
        // 行内重命名由 egui 绘制输入框（上一代 版本会被底层画布纹理盖住）。
        let mut buffer = track.rename_buffer.clone();
        let input_id = egui::Id::unique(("lumino.track-rename", track.id));
        // 进入重命名的首帧抢一次焦点（否则按键仍落到 上一代，输入进不来）。
        let focused = egui::Id::unique(("lumino.track-rename-focus", track.id));
        if !ui
            .ctx()
            .data(|d| d.get_temp::<bool>(focused).unwrap_or(false))
        {
            ui.ctx().data_mut(|d| d.insert_temp(focused, true));
            name_ui.memory_mut(|m| m.request_focus(input_id));
        }
        let response = name_ui.add(
            egui::TextEdit::singleline(&mut buffer)
                .id(input_id)
                .font(FontId::proportional(theme::font::LABEL))
                .margin(Vec2::new(2.0, 0.0))
                .desired_width(name_rect.width().max(40.0)),
        );
        if response.changed() {
            panel_action::push_panel_action(
                ui.ctx(),
                SidebarPanelAction::RenameChanged {
                    track_id: track.id,
                    value: buffer,
                },
            );
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            panel_action::push_panel_action(
                ui.ctx(),
                SidebarPanelAction::RenameConfirmed(track.id),
            );
        }
    } else {
        // 退出重命名：清掉「已抢焦点」标记，下次进入可再次聚焦。
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique(("lumino.track-rename-focus", track.id)),
                false,
            )
        });
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
    }

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
    // 命中区在行与名称标签之后注册：egui 后注册者优先命中，
    // 避免名称 Label 与 S 的 6px 交叠把点击吃掉。
    let solo = ui.interact(
        solo_rect,
        egui::Id::unique(("lumino.track-solo", track.id)),
        Sense::click(),
    );
    let solo = crate::widgets::hover_tip(solo, solo_tip);
    if solo.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if solo.clicked() {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::TrackSoloToggled(track.id));
    }
    response
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

/// 行底色：与 上一代 `track_button_background` 同规则。
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
