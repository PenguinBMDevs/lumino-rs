//! 「更多」溢出菜单：把折叠掉的分组/工具以图标网格补偿。
//!
//! 内容由生产 `compute_overflow` + `overflow_menu_items` 镜像而来，顺序即动作下标；
//! 点击回传 [`ToolbarAction::OverflowItem`]，由生产按**同一视口宽度**重算解析。

use egui::{CornerRadius, Id, Pos2, Rect, Vec2};

use super::ToolbarModel;
use super::menu;
use super::toolbar_popup::{self, ToolbarAction};
use crate::icons::IconCache;
use crate::theme;

/// 网格按钮边长（生产 `BUTTON_SIZE`）。
const BTN: f32 = 40.0;
/// 图标尺寸（生产 `ICON_SIZE`）。
const ICON: f32 = 20.0;
/// 按钮间距（生产 `BUTTON_SPACING`）。
const SPACING: f32 = 4.0;
/// 面板内边距（生产 `PANEL_PADDING`）。
const PAD: f32 = 8.0;
/// 面板圆角（生产 `Border::rounded(8)`）。
const RADIUS: u8 = 8;

const TRIGGER_ID: &str = "lumino.overflow.trigger";

/// 「更多」按钮记录触发矩形；`band` 是工具栏 band（用于右对齐 / 下沿定位）。
pub fn set_trigger(ctx: &egui::Context, more: Rect, band: Rect) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(TRIGGER_ID), Some((more, band))));
}

/// 「更多」按钮不可见时清掉矩形（否则残留会吞掉 旧版 点击）。
pub fn clear_trigger(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<Option<(Rect, Rect)>>(Id::unique(TRIGGER_ID)));
}

/// 便于测试 / 弹层定位。
pub fn trigger(ctx: &egui::Context) -> Option<(Rect, Rect)> {
    ctx.data(|d| d.get_temp::<Option<(Rect, Rect)>>(Id::unique(TRIGGER_ID)))
        .flatten()
}

/// 面板矩形：右对齐窗口右缘内缩 4、顶边贴工具栏下沿（生产 `positioned_overflow_menu`）。
pub fn panel_rect(band: Rect, items: usize) -> Rect {
    let count = items.max(1);
    let cols = (count as f32).sqrt().ceil() as usize;
    let rows = count.div_ceil(cols);
    let width = cols as f32 * BTN + cols.saturating_sub(1) as f32 * SPACING + PAD * 2.0;
    let height = rows as f32 * BTN + rows.saturating_sub(1) as f32 * SPACING + PAD * 2.0;
    Rect::from_min_size(
        Pos2::new(band.right() - 4.0 - width, band.bottom()),
        Vec2::new(width, height),
    )
}

/// 面板底色：工具栏底色压暗 10%（生产 `panel_background`）。
fn panel_bg() -> egui::Color32 {
    let base = theme::panel_bg();
    let dim = |c: u8| (c as f32 * 0.9) as u8;
    egui::Color32::from_rgb(dim(base.r()), dim(base.g()), dim(base.b()))
}

/// 在整窗 root 末尾画溢出网格，并发布占用矩形。
pub fn show_overlay(ui: &mut egui::Ui, model: &ToolbarModel, icons: &mut IconCache) {
    // 触发区必须**始终**占用：否则「更多」的点击会同时落到下层 旧版，
    // `ToggleOverflowMenu` 触发两次（开→关），面板只闪一帧。
    let Some((more, band)) = trigger(ui.ctx()) else {
        return;
    };
    let mut blocked = vec![more];

    if let Some(menu) = &model.overflow_menu
        && !menu.items.is_empty()
    {
        let panel = panel_rect(band, menu.items.len());
        blocked.push(panel);

        // 独立图层：面板覆盖区域内不再命中下层工具栏 / 卷帘。
        super::floating_panel(ui.ctx(), "lumino.overflow-panel", panel, |ui| {
            ui.painter().rect(
                panel,
                RADIUS,
                panel_bg(),
                egui::Stroke::NONE,
                egui::StrokeKind::Inside,
            );

            let count = menu.items.len();
            let cols = (count as f32).sqrt().ceil() as usize;
            let rows = count.div_ceil(cols);
            let pointer = ui.input(|i| i.pointer.interact_pos());
            for (index, item) in menu.items.iter().enumerate() {
                let col = index / rows;
                let row = index % rows;
                let cell = Rect::from_min_size(
                    Pos2::new(
                        panel.left() + PAD + col as f32 * (BTN + SPACING),
                        panel.top() + PAD + row as f32 * (BTN + SPACING),
                    ),
                    Vec2::splat(BTN),
                );
                if item.enabled && pointer.is_some_and(|pos| cell.contains(pos)) {
                    // 生产悬停色：白色 12%。
                    ui.painter().rect_filled(
                        cell,
                        CornerRadius::same(6),
                        egui::Color32::from_white_alpha(31),
                    );
                }
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(cell).layout(
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                ));
                let image = icons.default_image(ui.ctx(), item.icon, ICON);
                let response = child.add(egui::Button::image(image).frame(false));
                let response = response.on_hover_text(&item.tooltip);
                if response.clicked() && item.enabled {
                    toolbar_popup::push_action(ui.ctx(), ToolbarAction::OverflowItem(index));
                }
            }

            // 点击面板/触发按钮以外关闭；Esc 同样关闭（与 旧版 的默认关闭层一致）。
            let ctx = ui.ctx();
            if !toolbar_popup::action_pending(ctx)
                && ui.input(|i| i.pointer.any_click())
                && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
                && !blocked.iter().any(|rect| rect.contains(pos))
            {
                toolbar_popup::push_action(ctx, ToolbarAction::CloseOverflowMenu);
            }
            if !toolbar_popup::action_pending(ctx) && ui.input(|i| i.key_pressed(egui::Key::Escape))
            {
                toolbar_popup::push_action(ctx, ToolbarAction::CloseOverflowMenu);
            }
            None
        });
    }

    menu::extend_blocked(ui.ctx(), blocked);
}
