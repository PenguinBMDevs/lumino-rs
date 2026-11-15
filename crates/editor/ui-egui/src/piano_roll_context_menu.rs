//! 钢琴卷帘右键上下文菜单（egui 版，图标 + 文字的行列布局）。
//!
//! 取代 旧版 `ui-editor::context_menu::view`：条目集与文案来自
//! `lumino_message::context_menu`（单一来源），这里只负责布局 / 命中 / 绘制。
//!
//! # 为什么不是图标格子（DRAW-046）
//!
//! 图标库只有 10 个右键菜单字形，而待确认图形有 27 个动作；实测 27 个槽位
//! 只用到 8 个字形，其中「齿轮」被 8 个动作复用。图标无法承担区分职责，
//! 因此改为 REAPER 式行式菜单：每行 = 图标 + 文字，列内自上而下、列间自左向右。
//!
//! # 布局
//!
//! - 列宽按该列最长文字的 galley 实测（CJK / 拉丁同一套度量，不靠字符数估算）；
//! - 行高 24，待确认图形 19 行 = 472px，仍在 720p 可用区内；
//! - 键盘：左右切列、上下列内移动、回车触发；置灰项不可点。

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};
use lumino_message::context_menu::{item_disabled_hint, item_enabled, item_label, menu_columns};
use lumino_message::{ContextMenuTarget, PianoRollContextMenuAction};

use crate::icons::IconCache;

mod icons;

use icons::item_icon;

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
/// 列间距。
const COLUMN_GAP: f32 = 16.0;
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
/// 正文色（面板固定深色，不跟随主题）。
const TEXT: Color32 = Color32::from_rgb(242, 242, 242);
/// 置灰文字色。
const TEXT_DISABLED: Color32 = Color32::from_rgb(120, 120, 126);
/// 置灰图标色。
const ICON_DISABLED: Color32 = Color32::from_rgb(96, 96, 102);
/// 键盘聚焦框色。
const FOCUS_RING: Color32 = Color32::from_rgb(120, 170, 255);

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

/// 一列的几何（`left` 为窗口坐标，`width` 为该列行宽）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ColumnLayout {
    left: f32,
    width: f32,
    rows: usize,
}

/// 本帧菜单几何（窗口坐标）。
#[derive(Debug, Clone, PartialEq)]
pub struct MenuLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 各列的左边界与行宽。
    columns: Vec<ColumnLayout>,
}

impl MenuLayout {
    /// 列数。
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }
}

/// 按列宽与行数放置面板（纯几何，可在无 `egui::Context` 的测试里直接调用）。
///
/// `widths` 是每列的行宽，`rows` 是每列的行数。
pub fn place(model: &PianoRollMenuModel, widths: &[f32], rows: &[usize]) -> MenuLayout {
    let count = widths.len().max(1) as f32;
    let content_w: f32 = widths.iter().sum::<f32>() + COLUMN_GAP * (count - 1.0);
    let max_rows = rows.iter().copied().max().unwrap_or(0) as f32;
    let size = Vec2::new(content_w + PADDING * 2.0, max_rows * ROW_H + PADDING * 2.0);

    let x = (model.position.x + OFFSET).clamp(
        model.available.left(),
        (model.available.right() - size.x - OFFSET).max(model.available.left()),
    );
    let below = model.position.y + OFFSET;
    let y = if below + size.y > model.available.bottom() {
        (model.position.y - size.y - OFFSET).max(model.available.top())
    } else {
        below
    };
    let y = y.min((model.available.bottom() - size.y).max(model.available.top()));
    let panel = Rect::from_min_size(egui::pos2(x, y), size);

    let mut columns = Vec::with_capacity(widths.len());
    let mut left = panel.left() + PADDING;
    for (index, width) in widths.iter().enumerate() {
        columns.push(ColumnLayout {
            left,
            width: *width,
            rows: rows.get(index).copied().unwrap_or(0),
        });
        left += width + COLUMN_GAP;
    }
    MenuLayout { panel, columns }
}

/// 第 `column` 列第 `row` 行的矩形（窗口坐标）；越界返回 [`Rect::NOTHING`]。
pub fn item_rect(layout: &MenuLayout, column: usize, row: usize) -> Rect {
    let Some(column) = layout.columns.get(column) else {
        return Rect::NOTHING;
    };
    if row >= column.rows {
        return Rect::NOTHING;
    }
    Rect::from_min_size(
        egui::pos2(
            column.left,
            layout.panel.top() + PADDING + row as f32 * ROW_H,
        ),
        Vec2::new(column.width, ROW_H),
    )
}

/// 按当前字体实测每列行宽（文字按 galley 度量，CJK / 拉丁都不截断）。
fn column_widths(ui: &egui::Ui, target: ContextMenuTarget) -> Vec<f32> {
    menu_columns(target)
        .iter()
        .map(|items| {
            let mut text = 0.0_f32;
            for item in *items {
                let galley = ui.painter().layout_no_wrap(
                    item_label(*item).to_owned(),
                    FontId::proportional(TEXT_SIZE),
                    TEXT,
                );
                text = text.max(galley.size().x);
            }
            ROW_PAD * 2.0 + ICON + ICON_GAP + text
        })
        .collect()
}

/// 计算本帧布局（需要 `ui` 来实测文字宽度）。
pub fn layout(ui: &egui::Ui, model: &PianoRollMenuModel) -> MenuLayout {
    let columns = menu_columns(model.target);
    let widths = column_widths(ui, model.target);
    let rows: Vec<usize> = columns.iter().map(|items| items.len()).collect();
    place(model, &widths, &rows)
}

const ACTION_ID: &str = "lumino.piano-roll-menu.action";
const FOCUS_ID: &str = "lumino.piano-roll-menu.focus";
const LAYOUT_ID: &str = "lumino.piano-roll-menu.layout";

/// 键盘焦点：位置变了就归零（同一位置重开保持）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct MenuFocus {
    position: Pos2,
    column: usize,
    row: usize,
    /// 是否用过键盘（鼠标浏览时不画聚焦框）
    keyboard: bool,
}

fn load_focus(ctx: &egui::Context, model: &PianoRollMenuModel) -> MenuFocus {
    ctx.data(|d| d.get_temp::<MenuFocus>(egui::Id::unique(FOCUS_ID)))
        .filter(|focus| focus.position == model.position)
        .unwrap_or(MenuFocus {
            position: model.position,
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
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧菜单动作。
pub fn take_action(ctx: &egui::Context) -> Option<PianoRollContextMenuAction> {
    ctx.data_mut(|d| {
        d.remove_temp::<Option<PianoRollContextMenuAction>>(egui::Id::unique(ACTION_ID))
    })
    .flatten()
}

/// 键盘导航：左右切列、上下列内移动、回车触发。
fn apply_keyboard(
    ctx: &egui::Context,
    target: ContextMenuTarget,
    focus: &mut MenuFocus,
    action: &mut Option<PianoRollContextMenuAction>,
) {
    let columns = menu_columns(target);
    if columns.is_empty() {
        return;
    }
    let (down, up, right, left, confirm) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::ArrowLeft),
            // 只认回车：空格在编辑器里另有语义（播放 / 停止），不能拿来确认菜单。
            i.key_pressed(egui::Key::Enter),
        )
    });
    if !(down || up || right || left || confirm) {
        return;
    }
    focus.keyboard = true;
    focus.column = focus.column.min(columns.len() - 1);
    if right && focus.column + 1 < columns.len() {
        focus.column += 1;
    }
    if left && focus.column > 0 {
        focus.column -= 1;
    }
    let len = columns[focus.column].len();
    focus.row = focus.row.min(len.saturating_sub(1));
    if down {
        focus.row = (focus.row + 1).min(len.saturating_sub(1));
    }
    if up {
        focus.row = focus.row.saturating_sub(1);
    }
    if confirm
        && let Some(item) = columns[focus.column].get(focus.row).copied()
        && item_enabled(target, item)
    {
        *action = Some(PianoRollContextMenuAction::ItemClicked(item));
    }
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
    let columns = menu_columns(model.target);
    let layout = layout(ui, model);
    if layout.panel.width() <= 0.0 || layout.panel.height() <= 0.0 {
        return None;
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(LAYOUT_ID), layout.clone()));
    let mut focus = load_focus(ui.ctx(), model);
    let mut action = None;
    // 独立图层：菜单覆盖区域内不再命中画布骨架（滚动条 / 框选层），
    // 否则悬停与点击会"穿透"到下层。
    crate::chrome::floating_panel(ui.ctx(), "lumino.piano-roll-menu", layout.panel, |ui| {
        let painter = ui.painter().with_clip_rect(model.available);
        painter.rect(
            layout.panel,
            CornerRadius::same(8),
            PANEL_BG,
            Stroke::new(0.0, Color32::TRANSPARENT),
            StrokeKind::Inside,
        );

        apply_keyboard(ui.ctx(), model.target, &mut focus, &mut action);

        for (column, items) in columns.iter().enumerate() {
            for (row, item) in items.iter().copied().enumerate() {
                let rect = item_rect(&layout, column, row);
                let enabled = item_enabled(model.target, item);
                let response = ui.interact(
                    rect,
                    egui::Id::unique(("lumino.piano-roll-menu", column, row)),
                    egui::Sense::click(),
                );
                let pressed = enabled && response.is_pointer_button_down_on();
                let hovered = enabled && response.hovered();
                if pressed {
                    painter.rect_filled(rect, CornerRadius::same(4), ROW_PRESSED);
                } else if hovered {
                    painter.rect_filled(rect, CornerRadius::same(4), ROW_HOVER);
                }

                let icon_rect = Rect::from_center_size(
                    egui::pos2(rect.left() + ROW_PAD + ICON * 0.5, rect.center().y),
                    Vec2::splat(ICON),
                );
                icons.paint_tinted(
                    &painter,
                    item_icon(item),
                    icon_rect,
                    Some(if enabled { TEXT } else { ICON_DISABLED }),
                );
                let galley = painter.layout_no_wrap(
                    item_label(item).to_owned(),
                    FontId::proportional(TEXT_SIZE),
                    if enabled { TEXT } else { TEXT_DISABLED },
                );
                painter.galley(
                    egui::pos2(
                        icon_rect.right() + ICON_GAP,
                        rect.center().y - galley.size().y * 0.5,
                    ),
                    galley,
                    Color32::WHITE,
                );

                // 文字已经可见，只有置灰项需要额外解释为什么点不了。
                let response = response.on_hover_cursor(if enabled {
                    egui::CursorIcon::PointingHand
                } else {
                    egui::CursorIcon::NotAllowed
                });
                let response = if enabled {
                    response
                } else {
                    response.on_hover_ui(|ui| {
                        egui::Frame::new()
                            .fill(Color32::from_rgba_premultiplied(20, 20, 26, 245))
                            .inner_margin(egui::Margin::symmetric(6, 3))
                            .corner_radius(CornerRadius::same(4))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(item_disabled_hint())
                                        .size(11.0)
                                        .color(Color32::from_rgb(150, 150, 158)),
                                );
                            });
                    })
                };
                if enabled && response.clicked() {
                    action = Some(PianoRollContextMenuAction::ItemClicked(item));
                }
            }
        }

        // 键盘聚焦框（只在用过方向键之后出现）。
        if focus.keyboard
            && columns
                .get(focus.column)
                .is_some_and(|items| focus.row < items.len())
        {
            let rect = item_rect(&layout, focus.column, focus.row).shrink(1.0);
            painter.rect_stroke(
                rect,
                CornerRadius::same(4),
                Stroke::new(1.5, FOCUS_RING),
                StrokeKind::Inside,
            );
        }

        // 点击画布空白区域关闭（与 旧版 背景层一致，仅左键）；面板内点击由行处理。
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
            && !layout.panel.contains(pos)
        {
            action = Some(PianoRollContextMenuAction::Close);
        }
        None
    });
    store_focus(ui.ctx(), focus);
    action
}

#[cfg(test)]
mod tests;
