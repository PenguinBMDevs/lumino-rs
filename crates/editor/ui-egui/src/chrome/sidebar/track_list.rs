//! 左侧音轨列表：编号 / 名称分列对齐。
//!
//! 几何来自生产 上一代 `sidebar/panel.rs` + `sidebar/panel/track_item.rs`：
//! - 面板内容宽 = panel_width - 6（拖拽手柄），列内边距 8，右侧 8px 滚动条槽；
//! - 行按钮水平内边距 10，行内容内边距 4（添加行为 6）；
//! - 第一列固定 36 宽（含 2px 左内边距），列间距 4 → 名称列起点 = 行左 + 54
//!   （指挥轨图标同列，保证所有名称纵向对齐）；
//! - 独奏 `S` 右对齐到内容右边界（独奏时用警告色），点击切换独奏；
//! - 音轨行高 35，添加行高 40。
//!
//! 单行（含 S 命中）在 [`row`]；本文件只保留列表容器与「添加音轨」行。

mod row;

use egui::{Align, Align2, CornerRadius, FontId, Layout, Rect, Sense, Vec2};

use super::super::SidebarModel;
use super::panel_action::{self, SidebarPanelAction};
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;

use row::track_row;

/// 渲染音轨列表（标题 + 行 + 添加音轨）。
pub(super) fn show_model(
    ui: &mut egui::Ui,
    content: Rect,
    model: &SidebarModel,
    icons: &mut IconCache,
) {
    let list_left = content.left() + 8.0;
    // 上一代 的滚动条不占布局宽度（内容溢出时覆盖在右侧 8px 上），
    // 因此行宽 = 面板内容宽 - 左右各 8px 内边距。
    let list_right = content.right() - 8.0;
    let list_top = content.top() + layout::TRACK_LIST_HEADER_H;

    ui.painter().text(
        egui::pos2(list_left, content.top() + 15.0),
        Align2::LEFT_CENTER,
        crate::text::truncate_to_width(
            ui,
            &model.track_list_title,
            12.0,
            (list_right - list_left).max(1.0),
        ),
        FontId::proportional(12.0),
        theme::text(),
    );

    // 面板空白右键：行在内层会上移（egui 后注册者优先命中），因此这里先注册背景层。
    let backdrop = ui.interact(
        content,
        egui::Id::unique("lumino.sidebar.panel-bg"),
        Sense::click(),
    );
    if backdrop.secondary_clicked() {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::OpenPanelMenu);
    }

    // 内容底色只铺「音轨行 + 添加行」的高度（与 上一代 的可滚动内容同高），
    // 面板其余区域仍透出面板底色——否则整块面板会比 上一代 多一层底色。
    let content_height = layout::TRACK_ROW_H * model.tracks.len() as f32 + layout::TRACK_ADD_ROW_H;
    let background = Rect::from_min_size(
        egui::pos2(list_left, list_top),
        Vec2::new(list_right - list_left, content_height),
    );
    ui.painter().rect_filled(background, 0, theme::base_bg());

    // 手工逐行布局（不再用 `TableBuilder`）：表格的行间距会把「添加音轨」行
    // 推到最后一行的下半部分（实测行距 ≈ 41 vs 行高 36 → 10px 重叠）。
    // 上一代 的 `column![...rows, add]` 是 0 间距，这里保持一致。
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
    let mut rows: Vec<(usize, Rect)> = Vec::new();
    let mut reorder_started: Option<usize> = None;
    let mut reorder_dragging: Option<usize> = None;
    let mut reorder_ended = false;
    egui::ScrollArea::vertical()
        .id_salt("lumino-track-list")
        .auto_shrink([false, false])
        .show(&mut list_ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for track in &model.tracks {
                let response = track_row(ui, track, icons, model.solo_tip);
                rows.push((track.id, response.rect));
                if response.drag_started() {
                    reorder_started = Some(track.id);
                }
                if response.dragged() {
                    reorder_dragging = Some(track.id);
                }
                if response.drag_stopped() {
                    reorder_ended = true;
                }
                if response.drag_stopped() {
                    reorder_dragging = Some(track.id);
                }
            }
            let (add_rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), layout::TRACK_ADD_ROW_H),
                Sense::hover(),
            );
            add_track_row(ui, add_rect, &model.add_track_label, icons);
        });

    // 拖拽排序：插入下标 = 指针上方的行数（含滚动与行的真实矩形）。
    let insert_index = |ui: &egui::Ui| -> Option<usize> {
        let pos = ui.ctx().pointer_latest_pos()?;
        Some(
            rows.iter()
                .filter(|(_, rect)| rect.center().y < pos.y)
                .count(),
        )
    };
    if let Some(track_id) = reorder_started {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::ReorderStarted(track_id));
    }
    if reorder_ended {
        if let Some(index) = insert_index(ui) {
            panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::ReorderEnded { index });
        }
    } else if reorder_dragging.is_some()
        && let Some(index) = insert_index(ui)
    {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::ReorderMoved { index });
    }

    // 插入指示线：3px 强调色，画在目标行上沿（末位画在最后一行下沿）。
    if let Some(reorder) = &model.reorder
        && let Some(index) = reorder.hover_index
    {
        let y = rows
            .get(index)
            .map(|(_, rect)| rect.top())
            .unwrap_or_else(|| {
                rows.last()
                    .map(|(_, rect)| rect.bottom())
                    .unwrap_or(list_top)
            });
        ui.painter().rect_filled(
            Rect::from_min_size(
                egui::pos2(list_left, y - 1.5),
                Vec2::new(list_right - list_left, 3.0),
            ),
            0,
            theme::accent_strong(),
        );
    }

    // 右键菜单（由生产状态驱动）：行菜单锚在该行上，空白菜单锚在触发点。
    if let Some(menu_model) = &model.context_menu
        && let Some((_, anchor)) = rows.iter().find(|(id, _)| *id == menu_model.track_id)
    {
        let action = super::menu::track_menu(ui, content, *anchor, menu_model, icons);
        super::menu::emit(ui.ctx(), action);
    }
    if let Some(menu_model) = &model.panel_menu {
        let action = super::menu::panel_menu(ui, content, menu_model, icons);
        super::menu::emit(ui.ctx(), action);
    }

    // 取色器 / 端口·通道选择器：同样锚定到目标行。
    if let Some(picker) = &model.color_picker
        && let Some((_, anchor)) = rows.iter().find(|(id, _)| *id == picker.track_id)
    {
        let action = super::picker::color_picker(ui, content, *anchor, picker);
        super::picker::emit(ui.ctx(), action);
    }
    if let Some(picker) = &model.numeric_picker
        && let Some((_, anchor)) = rows.iter().find(|(id, _)| *id == picker.track_id)
    {
        let action = super::picker::numeric_picker(ui, content, *anchor, picker);
        super::picker::emit(ui.ctx(), action);
    }
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

/// 「添加音轨」行：图标列 24 + 列间距 4。
fn add_track_row(ui: &mut egui::Ui, rect: Rect, label: &str, cache: &mut IconCache) {
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), theme::base_bg());
    // 命中区与绘制同源；点击只给 egui（否则 上一代 的添加按钮会再加一条音轨）。
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

    let label_x = inner_left + layout::TRACK_ADD_ICON_COL_W + layout::TRACK_COL_GAP;
    ui.painter().text(
        egui::pos2(label_x, rect.center().y),
        Align2::LEFT_CENTER,
        crate::text::truncate_to_width(
            ui,
            label,
            theme::font::LABEL,
            (rect.right() - label_x - 8.0).max(1.0),
        ),
        FontId::proportional(theme::font::LABEL),
        theme::text(),
    );
}
