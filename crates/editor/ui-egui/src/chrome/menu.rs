//! 标题栏菜单栏：标签 + 下拉面板 + 一级子菜单。
//!
//! 菜单树由生产 `titlebar::menu::menus()` 镜像而来，动作只携带**遍历序号**；
//! 点击后把序号写进 egui 数据，由生产侧解析成真正的 `Message`。
//!
//! 布局对齐 iced_aw：面板宽 200、菜单项 28 高、分隔线 9 高、面板底色
//! `background.base`、悬停 `background.weaker`。

use egui::{Align2, Color32, FontId, Id, Layout, Pos2, Rect, Sense, Stroke, Vec2};

use super::{MenuEntry, TitlebarModel};
use crate::layout;
use crate::theme;
use crate::widgets;

/// 下拉面板宽度（生产 `MENU_WIDTH`）。
const PANEL_W: f32 = 200.0;
/// 菜单项高度。
const ITEM_H: f32 = 28.0;
/// 分隔线高度（生产 `space(4) + 1px + space(4)`）。
const SEP_H: f32 = 9.0;
/// 面板上下内边距。
const PANEL_PAD_Y: f32 = 4.0;
/// 面板左右内边距（行矩形内缩）。
const PANEL_PAD_X: f32 = 4.0;
/// 文本左内边距。
const TEXT_PAD_X: f32 = 8.0;
/// 面板相对菜单标签的左偏移（生产 `Menu::offset(9.0)`）。
const PANEL_OFFSET_X: f32 = 9.0;
/// 菜单文本字号。
const FONT: f32 = 14.0;

const STATE_ID: &str = "lumino.menu.state";
const LABELS_ID: &str = "lumino.menu.labels";
const ACTION_ID: &str = "lumino.menu.action";
const BLOCKED_ID: &str = "lumino.menu.blocked";

/// 菜单交互状态（跨帧保存在 egui 内存里）。
#[derive(Debug, Clone, Default)]
struct MenuState {
    /// 当前展开的主菜单索引。
    open: Option<usize>,
    /// 当前展开是「悬停切换」打开的：随后点击该标签不应立即关闭。
    hover_opened: bool,
    /// 当前展开的子菜单项索引（`open` 菜单内）。
    submenu: Option<usize>,
    /// 上一帧子面板的矩形：指针停在里面时保持展开（否则斜向下移会闪断）。
    submenu_rect: Option<Rect>,
}

fn read_state(ctx: &egui::Context) -> MenuState {
    ctx.data(|d| d.get_temp(Id::unique(STATE_ID)))
        .unwrap_or_default()
}

fn write_state(ctx: &egui::Context, state: &MenuState) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(STATE_ID), state.clone()));
}

/// 取走本帧点击产生的动作序号。
pub fn take_action(ctx: &egui::Context) -> Option<usize> {
    ctx.data_mut(|d| d.remove_temp::<usize>(Id::unique(ACTION_ID)))
}

/// 取走本帧菜单占用的屏幕矩形（逻辑点），用于从 iced 事件队列里消费点击。
pub fn take_blocked(ctx: &egui::Context) -> Vec<Rect> {
    ctx.data_mut(|d| d.remove_temp::<Vec<Rect>>(Id::unique(BLOCKED_ID)))
        .unwrap_or_default()
}

/// 清空本帧的占用矩形（未绘制菜单时必须清空，避免陈旧矩形吞掉 iced 点击）。
pub fn clear_blocked(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<Vec<Rect>>(Id::unique(BLOCKED_ID)));
}

/// 追加本帧的占用矩形（标题栏菜单与工具栏弹层共用同一份清单）。
pub fn extend_blocked(ctx: &egui::Context, rects: impl IntoIterator<Item = Rect>) {
    let mut current: Vec<Rect> = ctx
        .data(|d| d.get_temp(Id::unique(BLOCKED_ID)))
        .unwrap_or_default();
    current.extend(rects);
    ctx.data_mut(|d| d.insert_temp(Id::unique(BLOCKED_ID), current));
}

/// 菜单标签列表：优先使用可交互菜单树，否则退回纯标签。
pub fn labels(model: &TitlebarModel) -> Vec<String> {
    match &model.menu {
        Some(bar) => bar.menus.iter().map(|m| m.label.clone()).collect(),
        None => model.menus.clone(),
    }
}

/// 画标签并记录矩形（在标题栏 band 内调用）。
///
/// 返回每个标签的屏幕矩形，供 [`show_overlay`] 定位面板与计算占用区域。
pub fn show_labels(ui: &mut egui::Ui, model: &TitlebarModel, widths: &[f32]) -> Vec<Rect> {
    let labels = labels(model);
    let interactive = model.menu.is_some();
    let mut state = read_state(ui.ctx());
    let mut rects = Vec::with_capacity(labels.len());

    for (index, (label, width)) in labels.iter().zip(widths).enumerate() {
        let selected = state.open == Some(index);
        let inner = ui.allocate_ui_with_layout(
            Vec2::new(*width, layout::TITLEBAR_H),
            Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                let hovered = interactive && ui.rect_contains_pointer(ui.max_rect());
                if hovered || selected {
                    ui.painter()
                        .rect_filled(ui.max_rect(), 4, theme::panel_bg());
                }
                // 用 Label 而不是纯 painter：保留可访问树里的菜单名。
                widgets::menu_label_sized(ui, label, selected, FONT)
            },
        );
        let rect = inner.response.rect;
        let response = inner.inner;

        if interactive {
            if response.clicked() {
                if selected && !state.hover_opened {
                    state.open = None;
                } else {
                    state.open = Some(index);
                }
                state.hover_opened = false;
                state.submenu = None;
                state.submenu_rect = None;
            } else if response.hovered() && state.open.is_some() && state.open != Some(index) {
                // 已经展开时，悬停到别的标签直接切换（与 iced 菜单一致）。
                state.open = Some(index);
                state.hover_opened = true;
                state.submenu = None;
                state.submenu_rect = None;
            }
        }
        rects.push(rect);
    }

    if interactive {
        write_state(ui.ctx(), &state);
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(Id::unique(LABELS_ID), rects.clone()));
    rects
}

/// 在整窗 root 上画下拉面板（不受标题栏 30px 裁剪）。
pub fn show_overlay(ui: &mut egui::Ui, model: &TitlebarModel) {
    let Some(bar) = &model.menu else {
        return;
    };
    let labels: Vec<Rect> = ui
        .ctx()
        .data(|d| d.get_temp(Id::unique(LABELS_ID)))
        .unwrap_or_default();
    let mut state = read_state(ui.ctx());
    if let Some(open) = state.open
        && (open >= bar.menus.len() || open >= labels.len())
    {
        state.open = None;
    }

    let mut blocked = labels.clone();
    if let Some(open) = state.open
        && let Some(menu) = bar.menus.get(open)
    {
        let origin = Pos2::new(
            labels[open].left() + PANEL_OFFSET_X,
            layout::TITLEBAR_H + 2.0,
        );
        let height = panel_height(&menu.entries);
        let panel = Rect::from_min_size(origin, Vec2::new(PANEL_W, height));
        draw_panel(ui, panel, &menu.entries, &mut state, &mut blocked, true);
    }

    // 点击面板/标签以外的地方关闭菜单。
    if ui.input(|i| i.pointer.any_click())
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        && !blocked.iter().any(|rect| rect.contains(pos))
    {
        state.open = None;
        state.hover_opened = false;
        state.submenu = None;
        state.submenu_rect = None;
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.open = None;
        state.hover_opened = false;
        state.submenu = None;
        state.submenu_rect = None;
    }

    // 标签也算菜单占用区：点在标签上时由 `show_labels` 切换，不应触发 iced。
    extend_blocked(ui.ctx(), blocked);
    ui.ctx()
        .data_mut(|d| d.insert_temp(Id::unique(STATE_ID), state));
}

fn panel_height(entries: &[MenuEntry]) -> f32 {
    let rows: f32 = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Separator => SEP_H,
            _ => ITEM_H,
        })
        .sum();
    rows + PANEL_PAD_Y * 2.0
}

/// 画一块面板；`top_level` 为 `true` 时允许展开一级子菜单。
fn draw_panel(
    ui: &mut egui::Ui,
    panel: Rect,
    entries: &[MenuEntry],
    state: &mut MenuState,
    blocked: &mut Vec<Rect>,
    top_level: bool,
) {
    blocked.push(panel);
    ui.painter().rect_filled(panel, 6.0, theme::card_bg());
    ui.painter().rect_stroke(
        panel,
        6.0,
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );

    let mut y = panel.top() + PANEL_PAD_Y;
    let mut submenu_rows: Vec<(usize, Rect)> = Vec::new();
    let mut hovered_sub: Option<usize> = None;
    let mut hovered_action = false;
    for (index, entry) in entries.iter().enumerate() {
        let height = match entry {
            MenuEntry::Separator => SEP_H,
            _ => ITEM_H,
        };
        let row = Rect::from_min_size(
            Pos2::new(panel.left() + PANEL_PAD_X, y),
            Vec2::new(panel.width() - PANEL_PAD_X * 2.0, height),
        );
        y += height;

        match entry {
            MenuEntry::Separator => {
                let line_y = row.center().y;
                ui.painter().line_segment(
                    [
                        Pos2::new(row.left() + 4.0, line_y),
                        Pos2::new(row.right() - 4.0, line_y),
                    ],
                    Stroke::new(1.0, theme::border()),
                );
            }
            MenuEntry::Action {
                label,
                enabled,
                action,
            } => {
                let response = ui.interact(
                    row,
                    Id::unique(("lumino-menu-item", panel.left() as i32, index)),
                    Sense::click(),
                );
                if response.hovered() && *enabled {
                    hovered_action = true;
                    ui.painter().rect_filled(row, 4, theme::panel_bg());
                }
                let color = if *enabled {
                    theme::text()
                } else {
                    theme::text_dim()
                };
                ui.painter().text(
                    Pos2::new(row.left() + TEXT_PAD_X, row.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(FONT),
                    color,
                );
                if response.clicked() && *enabled {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), *action));
                    state.open = None;
                    state.hover_opened = false;
                    state.submenu = None;
                    state.submenu_rect = None;
                }
            }
            MenuEntry::Submenu { label, .. } => {
                let response = ui.interact(
                    row,
                    Id::unique(("lumino-menu-submenu", panel.left() as i32, index)),
                    Sense::click(),
                );
                let hovered = response.hovered();
                if hovered {
                    ui.painter().rect_filled(row, 4, theme::panel_bg());
                }
                ui.painter().text(
                    Pos2::new(row.left() + TEXT_PAD_X, row.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(FONT),
                    theme::text(),
                );
                ui.painter().text(
                    Pos2::new(row.right() - TEXT_PAD_X, row.center().y),
                    Align2::RIGHT_CENTER,
                    "›",
                    FontId::proportional(FONT + 2.0),
                    theme::text_dim(),
                );
                submenu_rows.push((index, row));
                if hovered {
                    hovered_sub = Some(index);
                }
            }
        }
    }

    if !top_level {
        return;
    }
    // 点击动作项后菜单已关闭：不要再把子菜单写回状态。
    if state.open.is_none() {
        state.submenu = None;
        state.submenu_rect = None;
        return;
    }

    // 子菜单展开条件：悬停父行、或指针仍停在上一次的子面板里（跨越间隙不闪断）。
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let keep = state.submenu.filter(|index| {
        pointer.is_some_and(|pos| {
            state.submenu_rect.is_some_and(|rect| rect.contains(pos))
                || submenu_rows
                    .iter()
                    .any(|(row_index, row)| row_index == index && row.contains(pos))
        })
    });
    let mut target = hovered_sub.or(keep);
    if hovered_action {
        // 指针落在别的菜单项上：收起子菜单（与 iced 行为一致）。
        target = None;
    }
    state.submenu = target;

    let drawn = target.and_then(|index| {
        let Some(MenuEntry::Submenu { entries, .. }) = entries.get(index) else {
            return None;
        };
        let (_, row) = submenu_rows
            .iter()
            .find(|(row_index, _)| *row_index == index)?;
        let origin = Pos2::new(panel.right() - 2.0, row.top());
        let height = panel_height(entries);
        let sub = Rect::from_min_size(origin, Vec2::new(PANEL_W, height));
        draw_panel(ui, sub, entries, state, blocked, false);
        Some(sub)
    });
    state.submenu_rect = drawn;
}

/// 便于单测：当前展开的主菜单索引。
pub fn open_index(ctx: &egui::Context) -> Option<usize> {
    read_state(ctx).open
}

/// 便于单测：本帧菜单标签矩形（按标签顺序）。
pub fn label_rects(ctx: &egui::Context) -> Vec<Rect> {
    ctx.data(|d| d.get_temp(Id::unique(LABELS_ID)))
        .unwrap_or_default()
}

/// 便于单测：悬停高亮色（与生产 `background.weaker` 对齐）。
pub fn hover_bg() -> Color32 {
    theme::panel_bg()
}
