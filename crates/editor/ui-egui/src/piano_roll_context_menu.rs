//! 钢琴卷帘右键上下文菜单（egui 版，图标 + 文字的单竖列）。
//!
//! 取代 上一代 `ui-editor::context_menu::view`：条目集与文案来自
//! `lumino_message::context_menu`（单一来源），这里只负责布局 / 命中 / 绘制。
//!
//! # 布局
//!
//! 固定一列：常规动作在前，目标专属动作在后，组间画一条分隔线。
//! - 列宽按最长文字的 galley 实测（CJK / 拉丁同一套度量，不靠字符数估算）；
//! - 行高 24；待确认图形 27 行可能高于 720p 可用区 → 面板高度钳到可用区，
//!   面板内滚轮滚动并显示细滚动条，**不拆列、不隐藏条目**；
//! - 键盘：上用下移、回车触发；置灰项不可点（方向键左右不再切列）。
//!
//! # 为什么不是图标格子
//!
//! 图标库只有 10 个右键菜单字形，而待确认图形有 27 个动作；实测 27 个槽位
//! 只用到 8 个字形，其中「齿轮」被 8 个动作复用。图标无法承担区分职责，
//! 因此保留 REAPER 式行式菜单（每行 = 图标 + 文字），但收成单竖列。

use egui::{Color32, Pos2, Rect, Vec2};
use lumino_message::context_menu::{item_enabled, menu_groups};
use lumino_message::{
    ContextMenuAvailability, ContextMenuTarget, PianoRollContextMenuAction,
    PianoRollContextMenuItem as Item,
};

use crate::icons::IconCache;

mod geometry;
mod icons;
mod render;

pub use geometry::{MenuLayout, item_rect, layout, place};

/// 行高。
const ROW_H: f32 = 24.0;
/// 行内图标边长。
const ICON: f32 = 16.0;
/// 图标与文字的间距。
const ICON_GAP: f32 = 8.0;
/// 行内左右留白（图标之前 / 文字之后）。
const ROW_PAD: f32 = 6.0;
/// 面板内边距。
const PADDING: f32 = 8.0;
/// 组间分隔线占位（1px 线 + 上下各 4px）。
const SEPARATOR_H: f32 = 9.0;
/// 菜单文字字号。
const TEXT_SIZE: f32 = 12.0;
/// 弹出位置相对鼠标的偏移。
const OFFSET: f32 = 16.0;
/// 面板底色（0.06/0.06/0.08 @ 0.96）。
const PANEL_BG: Color32 = Color32::from_rgba_premultiplied(15, 15, 20, 245);
/// 悬浮底色（白色 12%，预乘）。
const ROW_HOVER: Color32 = Color32::from_rgba_premultiplied(31, 31, 31, 31);
/// 按下底色（白色 22%，预乘）。
const ROW_PRESSED: Color32 = Color32::from_rgba_premultiplied(56, 56, 56, 56);
/// 组间分隔线。
const SEPARATOR: Color32 = Color32::from_rgb(52, 52, 60);
/// 正文色（面板固定深色，不跟随主题）。
const TEXT: Color32 = Color32::from_rgb(242, 242, 242);
/// 置灰文字色。
const TEXT_DISABLED: Color32 = Color32::from_rgb(120, 120, 126);
/// 置灰图标色。
const ICON_DISABLED: Color32 = Color32::from_rgb(96, 96, 102);
/// 键盘聚焦框色。
const FOCUS_RING: Color32 = Color32::from_rgb(120, 170, 255);
/// 滚动条宽度。
const SCROLL_BAR_W: f32 = 3.0;
/// 滚动条与内容右缘的间距。
const SCROLL_BAR_GAP: f32 = 3.0;
/// 滚动条轨道色（白色 8%，预乘）。
const SCROLL_TRACK: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 20);
/// 滚动条滑块色（白色 26%，预乘）。
const SCROLL_THUMB: Color32 = Color32::from_rgba_premultiplied(66, 66, 66, 66);

/// 右键菜单模型（全部为窗口逻辑坐标）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PianoRollMenuModel {
    /// 右键按下的画布位置（已换算成窗口坐标）。
    pub position: Pos2,
    /// 可放置菜单的画布区域（窗口坐标）。
    pub available: Rect,
    /// 作用目标（决定条目集）。
    pub target: ContextMenuTarget,
    /// 编辑状态（决定无选中 / 无刀时是否置灰）。
    pub availability: ContextMenuAvailability,
}

/// 单列的全部条目（按分组顺序摊平）。
fn flat_items(target: ContextMenuTarget) -> Vec<Item> {
    menu_groups(target)
        .iter()
        .flat_map(|group| group.iter().copied())
        .collect()
}

const ACTION_ID: &str = "lumino.piano-roll-menu.action";
const FOCUS_ID: &str = "lumino.piano-roll-menu.focus";
const LAYOUT_ID: &str = "lumino.piano-roll-menu.layout";

/// 键盘焦点与滚动位置（位置 / 目标变了就归零；同一位置重开保持）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct MenuFocus {
    position: Pos2,
    target: ContextMenuTarget,
    row: usize,
    /// 是否用过键盘（鼠标浏览时不画聚焦框）
    keyboard: bool,
    /// 内容滚动偏移（像素）
    scroll: f32,
}

fn load_focus(ctx: &egui::Context, model: &PianoRollMenuModel) -> MenuFocus {
    ctx.data(|d| d.get_temp::<MenuFocus>(egui::Id::unique(FOCUS_ID)))
        .filter(|focus| focus.position == model.position && focus.target == model.target)
        .unwrap_or(MenuFocus {
            position: model.position,
            target: model.target,
            ..MenuFocus::default()
        })
}

fn store_focus(ctx: &egui::Context, focus: MenuFocus) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(FOCUS_ID), focus));
}

/// 本帧菜单几何（供测试与诊断取用）。
pub fn cached_layout(ctx: &egui::Context) -> Option<MenuLayout> {
    ctx.data(|d| d.get_temp::<MenuLayout>(egui::Id::unique(LAYOUT_ID)))
}

/// 记录本帧菜单动作（一帧一条；后写入覆盖）。
pub fn push_action(ctx: &egui::Context, action: PianoRollContextMenuAction) {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).push(ctx, action);
}

/// 取走本帧菜单动作。
pub fn take_action(ctx: &egui::Context) -> Option<PianoRollContextMenuAction> {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).take(ctx)
}

/// 指针在面板上时用滚轮滚动菜单（面板放得下时不拦截）。
fn apply_wheel(ctx: &egui::Context, layout: &MenuLayout, focus: &mut MenuFocus) {
    if layout.max_scroll() <= 0.0 {
        return;
    }
    let Some(pointer) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    if !layout.panel.contains(pointer) {
        return;
    }
    // 读取后清零（与 `ScrollArea` 同口径）：不消费的话，滚动量会在后续帧
    // 反复生效，滚一格就连滚 150ms。
    let delta = ctx.input_mut(|i| {
        let delta = i.smooth_scroll_delta.y;
        i.smooth_scroll_delta = Vec2::ZERO;
        delta
    });
    if delta != 0.0 {
        focus.scroll = (focus.scroll - delta).clamp(0.0, layout.max_scroll());
        // 焦点跟着视口走：滚轮之后按上下键应从看见的第一行继续，而不是跳回顶部。
        focus.row = layout.row_at(focus.scroll);
    }
}

/// 键盘导航：上下移动、回车触发；左右不再切列。
///
/// 返回焦点行是否被方向键移动（只有移动时才需要把焦点行滚进视口；
/// 否则滚轮滚动会被"焦点行在视口外"误判成需要回滚）。
fn apply_keyboard(
    ctx: &egui::Context,
    target: ContextMenuTarget,
    availability: ContextMenuAvailability,
    items: &[Item],
    focus: &mut MenuFocus,
    action: &mut Option<PianoRollContextMenuAction>,
) -> bool {
    if items.is_empty() {
        return false;
    }
    let (down, up, confirm) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowUp),
            // 只认回车：空格在编辑器里另有语义（播放 / 停止），不能拿来确认菜单。
            i.key_pressed(egui::Key::Enter),
        )
    });
    if !(down || up || confirm) {
        return false;
    }
    focus.keyboard = true;
    focus.row = focus.row.min(items.len() - 1);
    if down {
        focus.row = (focus.row + 1).min(items.len() - 1);
    }
    if up {
        focus.row = focus.row.saturating_sub(1);
    }
    if confirm
        && let Some(item) = items.get(focus.row).copied()
        && item_enabled(target, item, availability)
    {
        *action = Some(PianoRollContextMenuAction::ItemClicked(item));
    }
    down || up
}

/// 键盘移动后把焦点行滚进视口。
fn ensure_visible(layout: &MenuLayout, row: usize, scroll: &mut f32) {
    let Some(top) = layout.row_top(row) else {
        return;
    };
    let viewport = layout.content.height();
    if top < *scroll {
        *scroll = top;
    } else if top + ROW_H > *scroll + viewport {
        *scroll = top + ROW_H - viewport;
    }
    *scroll = scroll.clamp(0.0, layout.max_scroll());
}

/// 点击画布空白区域关闭（与画布背景层一致，仅左键）；面板内点击由行处理。
fn clicked_outside(ctx: &egui::Context, model: &PianoRollMenuModel, layout: &MenuLayout) -> bool {
    // 注意：**不能**用 `any_click()`——右键开菜单的那一次右键会被重放进本帧，
    // 菜单会「刚打开就被自己关掉」。
    ctx.input(|input| {
        input
            .pointer
            .primary_clicked()
            .then(|| input.pointer.interact_pos())
            .flatten()
            .is_some_and(|pos| model.available.contains(pos) && !layout.panel.contains(pos))
    })
}

/// 绘制菜单并返回本帧动作。
///
/// `available` 内的点击若落在面板外 = 关闭（与 上一代 `background_close_overlay`
/// 同语义）；调用方应把 `available` 加入占用矩形，避免同一次点击又落到画布。
pub fn show(
    ui: &egui::Ui,
    model: &PianoRollMenuModel,
    icons: &mut IconCache,
) -> Option<PianoRollContextMenuAction> {
    let items = flat_items(model.target);
    let mut focus = load_focus(ui.ctx(), model);
    let mut layout = layout(ui, model, focus.scroll);
    if layout.panel.width() <= 0.0 || layout.panel.height() <= 0.0 {
        return None;
    }

    apply_wheel(ui.ctx(), &layout, &mut focus);
    let mut action = None;
    let moved = apply_keyboard(
        ui.ctx(),
        model.target,
        model.availability,
        &items,
        &mut focus,
        &mut action,
    );
    if moved {
        ensure_visible(&layout, focus.row, &mut focus.scroll);
    }
    layout.set_scroll(focus.scroll);

    // 独立图层：菜单覆盖区域内不再命中画布骨架（滚动条 / 框选层），
    // 否则悬停与点击会"穿透"到下层。
    let mut clicked = None;
    crate::chrome::floating_panel(ui.ctx(), "lumino.piano-roll-menu", layout.panel, |ui| {
        clicked = render::panel(ui, model, &items, &layout, &focus, icons);
        None
    });

    if action.is_none() {
        action = clicked;
    }
    if action.is_none() && clicked_outside(ui.ctx(), model, &layout) {
        action = Some(PianoRollContextMenuAction::Close);
    }
    store_focus(ui.ctx(), focus);
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(LAYOUT_ID), layout));
    action
}

#[cfg(test)]
mod tests;
