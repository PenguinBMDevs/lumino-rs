//! 左侧栏右键菜单：深色竖排图标 + 悬停提示。
//!
//! 几何与 旧版 `sidebar/context_menu.rs` / `panel_context_menu.rs` 一致：
//! 按钮 40×40、图标 36、间距 4、内边距 8（面板宽 56）、圆角 8、
//! 底色 `rgba(0.06, 0.06, 0.08, 0.96)`，悬停白色 12%。
//!
//! 菜单由生产状态驱动（`SidebarModel::context_menu` / `panel_menu`）；
//! 这里只负责绘制与命中，点击结果经 `panel_action::push_menu_action` 回传。

use egui::{Color32, CornerRadius, Rect, Sense, Stroke, Vec2};

use super::super::{SidebarMenuEntry, SidebarPanelMenuModel, SidebarTrackMenuModel};
use super::panel_action::{self, SidebarMenuAction};
use crate::icons::IconCache;

/// 按钮边长。
const BUTTON: f32 = 40.0;
/// 图标边长。
const ICON: f32 = 36.0;
/// 按钮间距。
const SPACING: f32 = 4.0;
/// 面板内边距。
const PAD: f32 = 8.0;
/// 面板宽度。
const MENU_W: f32 = BUTTON + PAD * 2.0;

/// 面板底色（`rgba(0.06, 0.06, 0.08, 0.96)`）。
fn menu_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(15, 15, 20, 245)
}

/// 悬停底色（白色 12%）。
fn hover_bg() -> Color32 {
    Color32::from_white_alpha(31)
}

/// 菜单高度。
fn menu_height(count: usize) -> f32 {
    count as f32 * BUTTON + count.saturating_sub(1) as f32 * SPACING + PAD * 2.0
}

/// 绘制一个菜单；返回被点条目下标。
fn show_menu(
    ui: &egui::Ui,
    id: &str,
    pos: egui::Pos2,
    entries: &[SidebarMenuEntry],
    icons: &mut IconCache,
) -> Option<usize> {
    if entries.is_empty() {
        return None;
    }
    let height = menu_height(entries.len());
    let mut clicked = None;
    egui::Area::new(egui::Id::unique(id))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(MENU_W, height), Sense::hover());
            ui.painter().rect(
                rect,
                CornerRadius::same(8),
                menu_bg(),
                Stroke::NONE,
                egui::StrokeKind::Inside,
            );
            for (index, entry) in entries.iter().enumerate() {
                let button = Rect::from_min_size(
                    egui::pos2(
                        rect.left() + PAD,
                        rect.top() + PAD + index as f32 * (BUTTON + SPACING),
                    ),
                    Vec2::splat(BUTTON),
                );
                let response =
                    ui.interact(button, ui.scope_id().with(("item", index)), Sense::click());
                if response.hovered() {
                    ui.painter()
                        .rect_filled(button, CornerRadius::same(4), hover_bg());
                }
                let icon_rect = Rect::from_center_size(button.center(), Vec2::splat(ICON));
                icons.paint(ui.painter(), entry.icon, icon_rect);
                // 悬停提示在右侧（与 旧版 `tooltip::Position::Right` 同侧）。
                let response = response.on_hover_text(&entry.label);
                if response.clicked() {
                    clicked = Some(index);
                }
            }
        });
    // 命中由 `Area` 图层独占（`layer_id_at`），不再登记全局占用表（Step 7）。
    clicked
}

/// 音轨行菜单：右对齐到面板内容右边界 −8，纵向对齐到该行上方 +8。
///
/// `content` = 面板内容矩形（用于背景关闭与占用），`anchor` = 目标行矩形。
pub(super) fn track_menu(
    ui: &egui::Ui,
    content: Rect,
    anchor: Rect,
    model: &SidebarTrackMenuModel,
    icons: &mut IconCache,
) -> Option<SidebarMenuAction> {
    let pos = egui::pos2(
        (content.right() - 8.0 - MENU_W).max(content.left()),
        anchor.top() + 8.0,
    );
    let clicked = show_menu(ui, "lumino.sidebar.track-menu", pos, &model.entries, icons);

    // 背景：点面板内空白 / 右键关闭（与 旧版 的全面板 backdrop 同义）。
    let backdrop = ui.interact(
        content,
        egui::Id::unique("lumino.sidebar.track-menu.backdrop"),
        Sense::click(),
    );
    // 菜单本体由 `show_menu` 放进独立 `Area`（`Order::Foreground`）：
    // `Host::egui_layer_owns_pos` 经 `layer_id_at` 已独占命中，无需再登记到
    // 全局占用表（Step 7：消灭第二份真相源）。
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if let Some(index) = clicked {
        Some(SidebarMenuAction::TrackItem {
            track_id: model.track_id,
            index,
        })
    } else if backdrop.clicked() || escape {
        Some(SidebarMenuAction::CloseTrackMenu)
    } else {
        None
    }
}

/// 面板空白菜单：右对齐、纵向贴触发点 +4。
pub(super) fn panel_menu(
    ui: &egui::Ui,
    content: Rect,
    model: &SidebarPanelMenuModel,
    icons: &mut IconCache,
) -> Option<SidebarMenuAction> {
    let pos = egui::pos2(
        (content.right() - 4.0 - MENU_W).max(content.left()),
        model.pos.y + 4.0,
    );
    let clicked = show_menu(ui, "lumino.sidebar.panel-menu", pos, &model.entries, icons);
    let backdrop = ui.interact(
        content,
        egui::Id::unique("lumino.sidebar.panel-menu.backdrop"),
        Sense::click(),
    );
    // 面板菜单同样是独立 `Area`，命中由 `layer_id_at` 独占（Step 7）。
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if let Some(index) = clicked {
        Some(SidebarMenuAction::PanelItem(index))
    } else if backdrop.clicked() || escape {
        Some(SidebarMenuAction::ClosePanelMenu)
    } else {
        None
    }
}

/// 便捷：把菜单动作写进通道（供 `track_list` 调用）。
pub(super) fn emit(ctx: &egui::Context, action: Option<SidebarMenuAction>) {
    if let Some(action) = action {
        panel_action::push_menu_action(ctx, action);
    }
}
