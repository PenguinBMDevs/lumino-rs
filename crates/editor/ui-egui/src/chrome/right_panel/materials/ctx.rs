//! 素材右键上下文菜单：覆盖关闭背景 + 图标面板（从 `materials.rs` 拆出）。

use egui::{Color32, CornerRadius, Id, Pos2, Rect, Sense, Vec2};

use super::super::super::model::{
    MaterialContextMenuItem, MaterialContextMenuModel, MaterialsPanelModel,
};
use super::super::widgets;
use super::super::{RightSidebarAction, push_action, record_rect};
use crate::icons::{Icon, IconCache};

const CTX_W: f32 = 56.0;
const CTX_H: f32 = 144.0;
const CTX_PAD: f32 = 8.0;
const CTX_BTN: f32 = 40.0;
const CTX_ICON: f32 = 36.0;
const CTX_GAP: f32 = 4.0;
const CTX_OFFSET: f32 = 4.0;

const CTX_BG: Color32 = Color32::from_rgba_premultiplied(15, 15, 20, 245);
const CTX_MASK: Color32 = Color32::from_rgba_premultiplied(15, 15, 20, 140);

/// 右键上下文菜单：覆盖关闭背景 + 图标面板。
pub(super) fn draw(
    ui: &mut egui::Ui,
    content: Rect,
    menu: &MaterialContextMenuModel,
    model: &MaterialsPanelModel,
    icons: &mut IconCache,
) {
    record_rect(ui.ctx(), "material:ctx:backdrop".to_owned(), content);
    let backdrop = ui.interact(content, Id::unique("material-ctx-backdrop"), Sense::click());

    // 面板定位：默认右下方，贴近右/下缘翻转到另一侧。
    let x = if menu.x + CTX_W + CTX_OFFSET * 2.0 <= content.width() {
        content.left() + menu.x + CTX_OFFSET
    } else {
        (content.left() + menu.x - CTX_W - CTX_OFFSET).max(content.left())
    };
    let y = if menu.y + CTX_H + CTX_OFFSET * 2.0 <= content.height() {
        content.top() + menu.y + CTX_OFFSET
    } else {
        (content.top() + menu.y - CTX_H - CTX_OFFSET).max(content.top())
    };
    let panel = Rect::from_min_size(Pos2::new(x, y), Vec2::new(CTX_W, CTX_H));
    ui.painter()
        .rect_filled(panel, CornerRadius::same(8), CTX_BG);

    let entries = [
        (
            MaterialContextMenuItem::Rename,
            Icon::PencilOutline,
            "material:ctx:rename",
            &model.context_labels.rename,
        ),
        (
            MaterialContextMenuItem::Delete,
            Icon::ContextMenuDelete,
            "material:ctx:delete",
            &model.context_labels.delete,
        ),
        (
            MaterialContextMenuItem::UploadToCloud,
            Icon::ContextMenuUploadToCloud,
            "material:ctx:upload",
            &model.context_labels.upload,
        ),
    ];
    let mut clicked = None;
    let mut item_y = panel.top() + CTX_PAD;
    for (item, icon, key, label) in entries {
        let button = Rect::from_min_size(
            Pos2::new(panel.left() + CTX_PAD, item_y),
            Vec2::splat(CTX_BTN),
        );
        record_rect(ui.ctx(), key.to_owned(), button);
        let response = ui.interact(
            button,
            Id::unique(key),
            if menu.can_edit {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if menu.can_edit && response.hovered() {
            ui.painter()
                .rect_filled(button, CornerRadius::same(6), Color32::from_white_alpha(31));
        }
        let icon_rect = Rect::from_center_size(button.center(), Vec2::splat(CTX_ICON));
        icons.paint(ui.painter(), icon, icon_rect);
        if !menu.can_edit {
            // 置灰：图标上覆盖半透明面板底色（与 旧版 同策略）。
            ui.painter()
                .rect_filled(icon_rect, CornerRadius::same(4), CTX_MASK);
        }
        let hit = menu.can_edit && response.clicked();
        response.on_hover_text(label);
        if hit {
            clicked = Some(item);
        }
        item_y += CTX_BTN + CTX_GAP;
    }

    if let Some(item) = clicked {
        push_action(
            ui.ctx(),
            RightSidebarAction::MaterialContextMenuItemClicked {
                index: menu.index,
                item,
            },
        );
    } else if backdrop.clicked() && widgets::pointer_pos(ui).is_some_and(|pos| !panel.contains(pos))
    {
        push_action(ui.ctx(), RightSidebarAction::MaterialContextMenuClosed);
    }
}
