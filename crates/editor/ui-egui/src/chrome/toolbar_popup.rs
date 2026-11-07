//! 工具栏弹层：精度（吸附）下拉 + 洋葱皮范围菜单。
//!
//! 两个弹层都由生产状态驱动（`ToolbarModel::snap_menu` / `onion_menu`），
//! 点击只回传 [`ToolbarAction`]；由生产侧映射成 `Message` 再走 `process_message`。
//!
//! 几何对齐 iced：面板宽 190、内边距 6、圆角 6、`background.weak` 底 +
//! `background.strong` 1px 边框；菜单项 13px 文本、悬停白色 10%。

use egui::{Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, Vec2};
use lumino_core::SnapSpec;

use super::ToolbarModel;
use super::menu;
use crate::theme;

/// 面板宽度（生产 `MENU_WIDTH`）。
const MENU_W: f32 = 190.0;
/// 面板内边距。
const PANEL_PAD: f32 = 6.0;
/// 菜单项高度。
const ITEM_H: f32 = 24.0;
/// 菜单项间距。
const ITEM_GAP: f32 = 2.0;
/// 洋葱皮菜单项高度（生产 `padding([4, 8])` + 13px 文本）。
const ONION_ITEM_H: f32 = 22.0;
/// 菜单文本字号。
const TEXT_SIZE: f32 = 13.0;
/// 吸附小图尺寸。
const GLYPH: f32 = 16.0;
/// 悬停底色（白色 10%）。
fn hover() -> Color32 {
    Color32::from_white_alpha(26)
}

const ACTION_ID: &str = "lumino.toolbar.action";
const SNAP_TRIGGER_ID: &str = "lumino.toolbar.snap-trigger";
const ONION_TRIGGER_ID: &str = "lumino.toolbar.onion-triggers";

/// 工具栏弹层产生的动作（不依赖 iced，生产侧负责翻译成 `Message`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    /// 开合精度下拉。
    ToggleSnapPicker,
    /// 选择吸附预设。
    SnapSpec(SnapSpec),
    /// 打开自定义精度对话框。
    SnapCustom,
    /// 关闭精度下拉（外部点击 / Esc）。
    CloseSnapPicker,
    /// 开/关洋葱皮。
    ToggleOnionSkin,
    /// 开合洋葱皮范围菜单。
    ToggleOnionMenu,
    /// 选择洋葱皮范围（`OnionSkinMode::all()` 的下标）。
    OnionMode(usize),
    /// 关闭洋葱皮范围菜单。
    CloseOnionMenu,
}

/// 记录本帧要回传给生产的动作。
pub fn push_action(ctx: &egui::Context, action: ToolbarAction) {
    // `remove_temp` 要求 `Default`：用 `Option` 承载，取用时展平。
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<ToolbarAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<ToolbarAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 清空动作（每帧开始调用，避免陈旧动作被重复消费）。
pub fn clear_action(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<Option<ToolbarAction>>(Id::unique(ACTION_ID)));
}

fn has_action(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Option<ToolbarAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
}

/// 工具栏 band 记录触发矩形（逻辑屏幕坐标）。
pub fn set_snap_trigger(ctx: &egui::Context, rect: Rect) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(SNAP_TRIGGER_ID), rect));
}

/// 工具栏 band 记录洋葱皮主按钮与下拉三角的矩形。
pub fn set_onion_triggers(ctx: &egui::Context, button: Rect, caret: Rect) {
    let previous: Option<(Rect, Rect)> = ctx.data(|d| d.get_temp(Id::unique(ONION_TRIGGER_ID)));
    if previous != Some((button, caret)) {
        tracing::debug!("工具栏洋葱皮触发区：button={button:?} caret={caret:?}");
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ONION_TRIGGER_ID), (button, caret)));
}

/// 便于测试 / 弹层定位：精度下拉的触发矩形。
pub fn snap_trigger(ctx: &egui::Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp(Id::unique(SNAP_TRIGGER_ID)))
}

/// 便于测试 / 弹层定位：洋葱皮（主按钮，下拉三角）矩形。
pub fn onion_triggers(ctx: &egui::Context) -> Option<(Rect, Rect)> {
    ctx.data(|d| d.get_temp(Id::unique(ONION_TRIGGER_ID)))
}

/// 精度下拉面板矩形（在触发框下方、左对齐）。
pub fn snap_panel_rect(trigger: Rect, items: usize, custom: bool) -> Rect {
    let rows = items as f32 + if custom { 1.0 } else { 0.0 };
    let height = PANEL_PAD * 2.0 + rows * ITEM_H + (rows - 1.0).max(0.0) * ITEM_GAP;
    Rect::from_min_size(
        Pos2::new(trigger.left(), trigger.bottom() + 2.0),
        Vec2::new(MENU_W, height),
    )
}

/// 洋葱皮菜单矩形（在三角下方、右对齐）。
pub fn onion_panel_rect(caret: Rect, items: usize) -> Rect {
    let rows = items as f32;
    let height = PANEL_PAD * 2.0 + rows * ONION_ITEM_H + (rows - 1.0).max(0.0) * ITEM_GAP;
    Rect::from_min_size(
        Pos2::new(caret.right() - MENU_W, caret.bottom() + 2.0),
        Vec2::new(MENU_W, height),
    )
}

/// 在整窗 root 末尾画两个弹层，并发布占用矩形。
pub fn show_overlay(ui: &mut egui::Ui, model: &ToolbarModel) {
    let snap_trigger = snap_trigger(ui.ctx());
    let onion_triggers = onion_triggers(ui.ctx());

    let mut blocked: Vec<Rect> = Vec::new();
    if let Some(rect) = snap_trigger {
        blocked.push(rect);
    }
    if let Some((button, caret)) = onion_triggers {
        blocked.push(button);
        blocked.push(caret);
    }

    if let (Some(menu), Some(trigger)) = (&model.snap_menu, snap_trigger) {
        let panel = snap_panel_rect(trigger, menu.items.len(), true);
        draw_snap_panel(ui, panel, menu, &mut blocked);
    }
    if let (Some(menu), Some((_, caret))) = (&model.onion_menu, onion_triggers) {
        let panel = onion_panel_rect(caret, menu.items.len());
        draw_onion_panel(ui, panel, menu, &mut blocked);
    }

    // 点击面板与触发区以外关闭；Esc 同样关闭（与 iced `DropDown::on_dismiss` 对齐）。
    let ctx = ui.ctx();
    if !has_action(ctx)
        && ui.input(|i| i.pointer.any_click())
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        && !blocked.iter().any(|rect| rect.contains(pos))
    {
        if model.snap_menu.is_some() {
            push_action(ctx, ToolbarAction::CloseSnapPicker);
        } else if model.onion_menu.is_some() {
            push_action(ctx, ToolbarAction::CloseOnionMenu);
        }
    }
    if !has_action(ctx) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if model.snap_menu.is_some() {
            push_action(ctx, ToolbarAction::CloseSnapPicker);
        } else if model.onion_menu.is_some() {
            push_action(ctx, ToolbarAction::CloseOnionMenu);
        }
    }

    menu::extend_blocked(ctx, blocked);
}

fn draw_panel(ui: &egui::Ui, rect: Rect) {
    ui.painter().rect(
        rect,
        6,
        theme::TOOLBAR_BG,
        Stroke::new(1.0, theme::SELECTED_BG),
        egui::StrokeKind::Inside,
    );
}

fn draw_snap_panel(
    ui: &mut egui::Ui,
    panel: Rect,
    menu: &super::SnapMenuModel,
    blocked: &mut Vec<Rect>,
) {
    blocked.push(panel);
    draw_panel(ui, panel);

    let mut y = panel.top() + PANEL_PAD;
    for (index, item) in menu.items.iter().enumerate() {
        let row = Rect::from_min_size(
            Pos2::new(panel.left() + PANEL_PAD, y),
            Vec2::new(MENU_W - PANEL_PAD * 2.0, ITEM_H),
        );
        y += ITEM_H + ITEM_GAP;
        let response = ui.interact(row, Id::unique(("lumino-snap-item", index)), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(row, 3, hover());
        }
        let glyph = Rect::from_min_size(
            Pos2::new(row.left() + 8.0, row.center().y - GLYPH / 2.0),
            Vec2::splat(GLYPH),
        );
        snap_glyph(ui, glyph, item.spec, theme::TEXT_ON_WEAK);
        ui.painter().text(
            Pos2::new(glyph.right() + 6.0, row.center().y),
            Align2::LEFT_CENTER,
            &item.label,
            FontId::proportional(TEXT_SIZE),
            theme::TEXT_ON_WEAK,
        );
        if response.clicked() {
            push_action(ui.ctx(), ToolbarAction::SnapSpec(item.spec));
        }
    }

    // 自定义项（生产固定英文 `Custom…`，与 iced 一致）。
    let row = Rect::from_min_size(
        Pos2::new(panel.left() + PANEL_PAD, y),
        Vec2::new(MENU_W - PANEL_PAD * 2.0, ITEM_H),
    );
    let response = ui.interact(row, Id::unique("lumino-snap-custom"), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(row, 3, hover());
    }
    let glyph = Rect::from_min_size(
        Pos2::new(row.left() + 8.0, row.center().y - GLYPH / 2.0),
        Vec2::splat(GLYPH),
    );
    snap_glyph(
        ui,
        glyph,
        SnapSpec::Custom { ticks: 1 },
        theme::TEXT_ON_WEAK,
    );
    ui.painter().text(
        Pos2::new(glyph.right() + 6.0, row.center().y),
        Align2::LEFT_CENTER,
        &menu.custom_label,
        FontId::proportional(TEXT_SIZE),
        theme::TEXT_ON_WEAK,
    );
    if response.clicked() {
        push_action(ui.ctx(), ToolbarAction::SnapCustom);
    }
}

fn draw_onion_panel(
    ui: &mut egui::Ui,
    panel: Rect,
    menu: &super::OnionMenuModel,
    blocked: &mut Vec<Rect>,
) {
    blocked.push(panel);
    draw_panel(ui, panel);

    let mut y = panel.top() + PANEL_PAD;
    for (index, item) in menu.items.iter().enumerate() {
        let row = Rect::from_min_size(
            Pos2::new(panel.left() + PANEL_PAD, y),
            Vec2::new(MENU_W - PANEL_PAD * 2.0, ONION_ITEM_H),
        );
        y += ONION_ITEM_H + ITEM_GAP;
        let response = ui.interact(
            row,
            Id::unique(("lumino-onion-item", index)),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter().rect_filled(row, 3, hover());
        }
        // 勾选位固定占宽，避免选中态左右跳动。
        let check = Rect::from_min_size(
            Pos2::new(row.left() + 8.0, row.center().y - 8.0),
            Vec2::splat(16.0),
        );
        if item.selected {
            // 矢量勾：不依赖字体（egui 默认字体没有 U+2713）。
            let stroke = Stroke::new(1.6, theme::TEXT_ON_WEAK);
            let x = check.left() + 3.0;
            let y = check.center().y;
            ui.painter()
                .line_segment([Pos2::new(x, y), Pos2::new(x + 3.5, y + 3.5)], stroke);
            ui.painter().line_segment(
                [Pos2::new(x + 3.5, y + 3.5), Pos2::new(x + 10.0, y - 4.0)],
                stroke,
            );
        }
        ui.painter().text(
            Pos2::new(check.right() + 6.0, row.center().y),
            Align2::LEFT_CENTER,
            &item.label,
            FontId::proportional(TEXT_SIZE),
            theme::TEXT_ON_WEAK,
        );
        if response.clicked() {
            push_action(ui.ctx(), ToolbarAction::OnionMode(index));
        }
    }
}

/// 16×16 吸附规格矢量小图（移植生产 `toolbar/view/snap_icon.rs`）。
fn snap_glyph(ui: &egui::Ui, rect: Rect, spec: SnapSpec, color: Color32) {
    let fill = |x: f32, y: f32, w: f32, h: f32| {
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(rect.left() + x, rect.top() + y), Vec2::new(w, h)),
            0,
            color,
        );
    };
    match spec {
        SnapSpec::Bar => {
            fill(3.0, 3.0, 1.5, 10.0);
            fill(11.5, 3.0, 1.5, 10.0);
            fill(3.0, 3.0, 10.0, 1.2);
        }
        SnapSpec::Custom { .. } => {
            for x in [3.0, 7.0, 11.0] {
                fill(x, 3.0, 2.0, 1.2);
                fill(x, 11.8, 2.0, 1.2);
            }
            fill(3.0, 3.0, 1.2, 10.0);
            fill(11.8, 3.0, 1.2, 10.0);
        }
        SnapSpec::Note { notes, dots, div } => {
            let head = Pos2::new(rect.left() + 6.0, rect.top() + 11.0);
            if notes <= 2 {
                ui.painter()
                    .circle_stroke(head, 2.3, Stroke::new(1.2, color));
            } else {
                ui.painter().circle_filled(head, 2.3, color);
            }
            if notes != 1 {
                fill(8.1, 2.5, 1.2, 8.5);
            }
            if notes >= 8 {
                let flags = match notes {
                    8 => 1,
                    16 => 2,
                    _ => 3,
                };
                for index in 0..flags {
                    fill(9.0, 3.0 + index as f32 * 2.0, 4.0, 1.1);
                }
            }
            for index in 0..dots.min(2) {
                fill(12.0 + f32::from(index) * 2.0, 10.0, 1.5, 1.5);
            }
            if div > 1 {
                for index in 0..3 {
                    fill(9.5 + index as f32 * 2.0, 1.5, 1.0, 4.0);
                }
            }
        }
    }
}
