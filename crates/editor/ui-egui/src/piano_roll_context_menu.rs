//! 钢琴卷帘右键上下文菜单（egui 版）。
//!
//! 取代 旧版 `ui-editor::context_menu::view`：条目集与 tooltip 文案来自
//! `lumino_message::context_menu`（单一来源），这里只负责图标条 / 几何 / 命中。
//!
//! 几何与 旧版 原实现逐项一致：面板宽 56（40 按钮 + 8 内边距 ×2）、行距 4、
//! 面板圆角 8、按钮圆角 4、弹出偏移 16；下方放不下时翻到上方。

use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Vec2};
use lumino_message::context_menu::{item_label, menu_items};
use lumino_message::{ContextMenuTarget, PianoRollContextMenuAction};

use crate::icons::{Icon, IconCache};

/// 图标按钮边长。
const BUTTON: f32 = 40.0;
/// 图标绘制边长（与 旧版 `ICON_SIZE` 一致）。
const ICON: f32 = 36.0;
/// 按钮间距。
const SPACING: f32 = 4.0;
/// 面板内边距。
const PADDING: f32 = 8.0;
/// 面板宽度。
pub const PANEL_W: f32 = BUTTON + PADDING * 2.0;
/// 弹出位置相对鼠标的偏移。
const OFFSET: f32 = 16.0;
/// 面板底色（0.06/0.06/0.08 @ 0.96）。
const PANEL_BG: Color32 = Color32::from_rgba_premultiplied(15, 15, 20, 245);
/// 悬浮底色（白色 12%，预乘）。
const HOVER_BG: Color32 = Color32::from_rgba_premultiplied(31, 31, 31, 31);
/// 按下底色（白色 22%，预乘）。
const PRESSED_BG: Color32 = Color32::from_rgba_premultiplied(56, 56, 56, 56);

/// 右键菜单模型（全部为窗口逻辑坐标）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PianoRollMenuModel {
    /// 右键按下的画布位置（已换算成窗口坐标）。
    pub position: Pos2,
    /// 可放置菜单的画布区域（窗口坐标）。
    pub available: Rect,
    /// 作用目标（决定条目集）。
    pub target: ContextMenuTarget,
}

/// 按模型计算面板矩形（与 旧版 `view` 的夹取规则一致）。
pub fn panel_rect(model: &PianoRollMenuModel) -> Rect {
    let count = menu_items(model.target).len();
    let height = count as f32 * BUTTON + count.saturating_sub(1) as f32 * SPACING + PADDING * 2.0;
    let x = (model.position.x + OFFSET).clamp(
        model.available.left(),
        (model.available.right() - PANEL_W - OFFSET).max(model.available.left()),
    );
    let below = model.position.y + OFFSET;
    let y = if below + height > model.available.bottom() {
        (model.position.y - height - OFFSET).max(model.available.top())
    } else {
        below
    };
    let y = y.min((model.available.bottom() - height).max(model.available.top()));
    Rect::from_min_size(egui::pos2(x, y), Vec2::new(PANEL_W, height))
}

/// 菜单项 → egui 图标（与 旧版 原映射一致）。
fn item_icon(item: lumino_message::PianoRollContextMenuItem) -> Icon {
    use lumino_message::PianoRollContextMenuItem as Item;
    match item {
        Item::Cut | Item::SplitJoinedShape | Item::Straighten | Item::SliceCut => {
            Icon::ContextMenuCut
        }
        Item::Copy
        | Item::LiveShape
        | Item::SymmetryVertical
        | Item::BetweenCreate
        | Item::CopyShapeText => Icon::ContextMenuCopy,
        Item::Paste
        | Item::SplitLiveShape
        | Item::SymmetryHorizontal
        | Item::CopyAlong
        | Item::PasteShapeText => Icon::ContextMenuPaste,
        Item::Delete | Item::DeleteDrawnShape | Item::StraightenOff => Icon::ContextMenuDelete,
        Item::SelectAll | Item::MergeShapes | Item::VelocityUp => Icon::ContextMenuSelectAll,
        Item::BatchEdit
        | Item::JoinShapes
        | Item::EditShape
        | Item::PaintAreas
        | Item::MaskToggle
        | Item::MaskSettings
        | Item::ShapeList => Icon::Gear,
        Item::MouldShapes => Icon::ContextMenuColorPalette,
        Item::RoundCorners => Icon::Gear,
        Item::ReturnToDrawer => Icon::Undo,
    }
}

const ACTION_ID: &str = "lumino.piano-roll-menu.action";

/// 记录本帧菜单动作（一帧一条；后写入覆盖）。
pub fn push_action(ctx: &egui::Context, action: PianoRollContextMenuAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧菜单动作。
pub fn take_action(ctx: &egui::Context) -> Option<PianoRollContextMenuAction> {
    ctx.data_mut(|d| {
        d.remove_temp::<Option<PianoRollContextMenuAction>>(egui::Id::unique(ACTION_ID))
    })
    .flatten()
}

/// 绘制菜单并返回本帧动作。
///
/// `available` 内的点击若落在面板外 = 关闭（与 旧版 `background_close_overlay`
/// 同语义）；调用方应把 `available` 加入占用矩形，避免同一次点击又落到画布。
pub fn show(
    ui: &egui::Ui,
    model: &PianoRollMenuModel,
    icons: &mut IconCache,
) -> Option<PianoRollContextMenuAction> {
    let panel = panel_rect(model);
    if panel.width() <= 0.0 || panel.height() <= 0.0 {
        return None;
    }
    let painter = ui.painter().with_clip_rect(model.available);
    painter.rect(
        panel,
        CornerRadius::same(8),
        PANEL_BG,
        Stroke::new(0.0, Color32::TRANSPARENT),
        StrokeKind::Inside,
    );

    let items = menu_items(model.target);
    let mut action = None;
    let mut y = panel.top() + PADDING;
    for (index, item) in items.iter().enumerate() {
        let rect = Rect::from_min_size(egui::pos2(panel.left() + PADDING, y), Vec2::splat(BUTTON));
        let response = ui.interact(
            rect,
            egui::Id::unique(("lumino.piano-roll-menu", index)),
            egui::Sense::click(),
        );
        let background = if response.is_pointer_button_down_on() {
            PRESSED_BG
        } else if response.hovered() {
            HOVER_BG
        } else {
            Color32::TRANSPARENT
        };
        if background != Color32::TRANSPARENT {
            painter.rect_filled(rect, CornerRadius::same(4), background);
        }
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(ICON));
        icons.paint_tinted(&painter, item_icon(*item), icon_rect, Some(Color32::WHITE));
        // 面板固定深色：tooltip 也用深底浅字（与 旧版 迁移前一致）。
        let label = item_label(*item);
        let response = response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_ui(|ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgba_premultiplied(20, 20, 26, 245))
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .corner_radius(CornerRadius::same(4))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(label)
                                .size(12.0)
                                .color(Color32::from_rgb(242, 242, 242)),
                        );
                    });
            });
        if response.clicked() {
            action = Some(PianoRollContextMenuAction::ItemClicked(*item));
        }
        y += BUTTON + SPACING;
    }

    // 点击画布空白区域关闭（与 旧版 背景层一致，仅左键）；面板内点击由按钮处理。
    // 注意：**不能**用 `any_click()`——右键开菜单的那一次右键会被重放进本帧，
    // 菜单会「刚打开就被自己关掉」。
    if action.is_none()
        && let Some(pos) = ui.ctx().input(|input| {
            input
                .pointer
                .primary_clicked()
                .then(|| input.pointer.interact_pos())
                .flatten()
        })
        && model.available.contains(pos)
        && !panel.contains(pos)
    {
        action = Some(PianoRollContextMenuAction::Close);
    }

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(target: ContextMenuTarget) -> PianoRollMenuModel {
        PianoRollMenuModel {
            position: Pos2::new(400.0, 300.0),
            available: Rect::from_min_size(Pos2::new(248.0, 102.0), Vec2::new(1200.0, 800.0)),
            target,
        }
    }

    #[test]
    fn panel_fits_inside_available_and_flips_up() {
        // 靠右下角：下方放不下 → 翻到上方，且不越界（用 8 行的 Notes 目标；
        // DrawnShape 的 20 行面板本就高于小窗口，与 旧版 一致允许向下溢出）。
        let mut m = model(ContextMenuTarget::Notes);
        m.position = Pos2::new(1400.0, 880.0);
        let rect = panel_rect(&m);
        assert!(m.available.contains_rect(rect), "{rect:?} 越界");
        assert!(rect.bottom() <= m.position.y, "应翻到鼠标上方");

        // 靠左上角：正常向下展开。
        let mut m = model(ContextMenuTarget::Notes);
        m.position = Pos2::new(260.0, 110.0);
        let rect = panel_rect(&m);
        assert!(rect.top() >= m.position.y, "应向下展开");
        assert!(m.available.contains_rect(rect));
    }

    #[test]
    fn item_counts_follow_shared_table() {
        assert_eq!(menu_items(ContextMenuTarget::Notes).len(), 8);
        assert_eq!(menu_items(ContextMenuTarget::DrawnShape).len(), 20);
        assert_eq!(menu_items(ContextMenuTarget::PendingDrawnShape).len(), 15);
    }
}
