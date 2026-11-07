//! 悬浮绘制工具条（胶囊）+ 锚定设置面板。
//!
//! 结构对齐生产 `root/draw_toolbar.rs`：胶囊 = 拖拽柄 + 分隔线 + 六个绘制工具 +
//! 分隔线 + 「粗细」+ 齿轮；尺寸 34px 按钮 / 22px 图标 / 4px 间距 / 5px 内边距，
//! 圆角 = 高度 / 2。设置面板锚定在触发按钮**上方**（顶部空间不足才下翻），
//! 水平居中于该按钮，宽 248、圆角 8。
//!
//! 拖拽复用生产状态机：起拖 / 移动 / 松手分别回传 `ToolPanelDrag*`，由生产侧
//! `tool_panel_offset` 累积位移并做「接近默认位自动吸附」。

mod panels;
mod text_panel;

use egui::{CornerRadius, Id, Pos2, Rect, Sense, Stroke, Vec2};

use super::tool_panel::panels::panel_height;
use super::toolbar_popup::{self, ToolbarAction};
use super::{ToolPanelItemKind, ToolPanelModel, ToolPanelPanelOwner, ToolPanelSettingsPanel};
use crate::icons::IconCache;
use crate::layout;
use crate::theme;

/// 单个图标按钮尺寸（逻辑点）。
const BUTTON: f32 = 34.0;
/// 图标尺寸。
const ICON: f32 = 22.0;
/// 按钮间距。
const SPACING: f32 = 4.0;
/// 胶囊内边距。
const PAD: f32 = 5.0;
/// 拖拽柄抓取区宽度。
const GRIP_W: f32 = 22.0;
/// 拖拽柄竖条宽度。
const BAR_W: f32 = 2.0;
/// 拖拽柄竖条高度。
const BAR_H: f32 = 12.0;
/// 两根竖条的间距。
const BAR_GAP: f32 = 3.0;
/// 分隔线高度（左右各内缩 6）。
const DIVIDER_H: f32 = BUTTON - 12.0;
/// 设置面板宽度（生产 `MENU_WIDTH`）。
const MENU_W: f32 = 248.0;

const TRIGGERS_ID: &str = "lumino.tool-panel.triggers";
const FILL_INPUT_ID: &str = "lumino.tool-panel.fill-input";

/// 本帧记录的触发矩形（胶囊 + 各按钮 + 卷帘可用区）。
#[derive(Debug, Clone)]
struct Triggers {
    /// 胶囊矩形（占用）。
    pill: Rect,
    /// 卷帘区（面板定位与越界吸附用）。
    area: Rect,
    /// 六个工具条目的矩形。
    items: Vec<(ToolPanelItemKind, Rect)>,
    /// 右端「粗细」按钮矩形。
    thickness: Rect,
}

/// 胶囊高度。
const PILL_H: f32 = BUTTON + PAD * 2.0;

/// 记录本帧胶囊与按钮矩形。
fn set_triggers(ctx: &egui::Context, triggers: Triggers) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(TRIGGERS_ID), Some(triggers)));
}

/// 工具条未渲染时清掉矩形（避免陈旧占用吞掉下层点击）。
pub(super) fn clear_triggers(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<Option<Triggers>>(Id::unique(TRIGGERS_ID)));
}

fn triggers(ctx: &egui::Context) -> Option<Triggers> {
    ctx.data(|d| d.get_temp::<Option<Triggers>>(Id::unique(TRIGGERS_ID)))
        .flatten()
}

/// 便于测试 / 定位：胶囊矩形。
pub fn pill_rect(ctx: &egui::Context) -> Option<Rect> {
    triggers(ctx).map(|t| t.pill)
}

/// 便于测试 / 定位：某个工具条目的矩形。
pub fn item_rect(ctx: &egui::Context, kind: ToolPanelItemKind) -> Option<Rect> {
    triggers(ctx).and_then(|t| {
        t.items
            .iter()
            .find(|(item, _)| *item == kind)
            .map(|(_, rect)| *rect)
    })
}

/// 便于测试 / 定位：右端「粗细」按钮矩形。
pub fn thickness_rect(ctx: &egui::Context) -> Option<Rect> {
    triggers(ctx).map(|t| t.thickness)
}

/// 分音符填充输入框矩形（供测试与真实窗口自动化定位）。
pub fn fill_input_rect(ctx: &egui::Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Option<Rect>>(Id::unique(FILL_INPUT_ID)))
        .flatten()
}

/// 填充面板记录输入框矩形（只在这个面板打开时由 `fill` 调用）。
pub(crate) fn set_fill_input_rect(ctx: &egui::Context, rect: Rect) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(FILL_INPUT_ID), Some(rect)));
}

/// 填充面板关闭时清掉输入框矩形。
pub(crate) fn clear_fill_input_rect(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<Option<Rect>>(Id::unique(FILL_INPUT_ID)));
}

/// 设置面板矩形（便于测试）：锚定在 owner 按钮上方，越界吸附 / 下翻。
pub fn panel_rect_for(ctx: &egui::Context, panel: &ToolPanelSettingsPanel) -> Option<Rect> {
    let triggers = triggers(ctx)?;
    let height = panel_height(panel);
    let owner = match panel.owner() {
        ToolPanelPanelOwner::Item(kind) => triggers
            .items
            .iter()
            .find(|(item, _)| *item == kind)
            .map(|(_, rect)| *rect)?,
        ToolPanelPanelOwner::Thickness => triggers.thickness,
    };
    Some(place_panel(triggers.area, owner, height))
}

fn place_panel(area: Rect, owner: Rect, height: f32) -> Rect {
    let x = (owner.center().x - MENU_W / 2.0)
        .clamp(area.left(), (area.right() - MENU_W).max(area.left()));
    let mut y = owner.top() - height - 2.0;
    if y < area.top() {
        y = owner.bottom() + 2.0;
    }
    Rect::from_min_size(Pos2::new(x, y), Vec2::new(MENU_W, height))
}

/// 在整窗 root 末尾绘制胶囊与面板，并发布占用矩形。
pub fn show_overlay(ui: &mut egui::Ui, model: &ToolPanelModel, icons: &mut IconCache) {
    clear_fill_input_rect(ui.ctx());
    let full = ui.max_rect();
    let area = Rect::from_min_max(
        Pos2::new(model.left_inset, model.top_inset),
        Pos2::new(full.right(), full.bottom() - layout::STATUSBAR_H),
    );
    if area.width() <= 0.0 || area.height() <= PILL_H {
        return;
    }

    let widths: f32 = model.items.iter().map(|_| BUTTON).sum::<f32>();
    let separators = 1.0 + 1.0;
    let width = PAD * 2.0
        + GRIP_W
        + separators
        + widths
        + BUTTON * 2.0
        + SPACING * (model.items.len() as f32 + 4.0);
    let center_x = area.center().x + model.offset.0;
    let pill = Rect::from_min_size(
        Pos2::new(
            center_x - width / 2.0,
            area.bottom() - model.offset.1 - PILL_H,
        ),
        Vec2::new(width, PILL_H),
    );

    // 胶囊底 + 描边 + 内容。
    ui.painter().rect(
        pill,
        CornerRadius::same((PILL_H / 2.0) as u8),
        theme::TOOLBAR_BG,
        Stroke::new(1.0, theme::SELECTED_BG),
        egui::StrokeKind::Inside,
    );

    // 胶囊拖动命中区必须先注册：egui 里后注册的控件在上层，若放到按钮之后
    // 会把所有按钮的点击吞掉（按压被胶囊抢走，按钮永远点不中）。
    let pill_resp = ui.interact(
        pill,
        Id::unique("lumino.tool-panel.pill"),
        Sense::click_and_drag(),
    );

    let mut items: Vec<(ToolPanelItemKind, Rect)> = Vec::with_capacity(model.items.len());
    let mut x = pill.left() + PAD;
    let y = pill.top() + PAD;

    // 拖拽柄：两根竖条。
    let grip = Rect::from_min_size(Pos2::new(x, y), Vec2::new(GRIP_W, BUTTON));
    let bars_total = BAR_W * 2.0 + BAR_GAP;
    let bar_x = grip.center().x - bars_total / 2.0;
    for index in 0..2 {
        let bar = Rect::from_min_size(
            Pos2::new(
                bar_x + index as f32 * (BAR_W + BAR_GAP),
                grip.center().y - BAR_H / 2.0,
            ),
            Vec2::new(BAR_W, BAR_H),
        );
        ui.painter()
            .rect_filled(bar, CornerRadius::same(1), theme::SELECTED_BG);
    }
    let grip_resp = ui.interact(
        grip,
        Id::unique("lumino.tool-panel.grip"),
        Sense::click_and_drag(),
    );
    if grip_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    x += GRIP_W + SPACING;

    // 分隔竖线。
    let divider = |ui: &egui::Ui, x: f32| {
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(x, y + (BUTTON - DIVIDER_H) / 2.0),
                Vec2::new(1.0, DIVIDER_H),
            ),
            0,
            theme::SELECTED_BG,
        );
    };
    divider(ui, x);
    x += 1.0 + SPACING;

    for entry in &model.items {
        let cell = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(BUTTON));
        let response = item_cell(
            ui,
            icons,
            entry.icon,
            cell,
            BUTTON / 2.0,
            entry.selected,
            ("lumino.tool-panel.item", entry.kind as u8),
        );
        let response = response.on_hover_text(&entry.tooltip);
        if response.clicked() {
            toolbar_popup::push_action(
                ui.ctx(),
                ToolbarAction::ToolPanelItem {
                    kind: entry.kind,
                    settings: entry.settings_request,
                },
            );
        }
        items.push((entry.kind, cell));
        x += BUTTON + SPACING;
    }

    divider(ui, x);
    x += 1.0 + SPACING;

    // 右端「粗细」+ 齿轮。
    let thickness = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(BUTTON));
    let resp = item_cell(
        ui,
        icons,
        crate::icons::Icon::Thickness,
        thickness,
        BUTTON / 2.0,
        model.thickness_selected,
        "lumino.tool-panel.thickness",
    );
    let resp = resp.on_hover_text(&model.thickness_tooltip);
    if resp.clicked() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ThicknessToggle);
    }
    x += BUTTON + SPACING;

    let gear = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(BUTTON));
    let resp = item_cell(
        ui,
        icons,
        crate::icons::Icon::Gear,
        gear,
        BUTTON / 2.0,
        false,
        "lumino.tool-panel.gear",
    );
    if resp.clicked() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::OpenDrawSettings);
    }

    // 胶囊拖动：优先拖拽柄，其次胶囊空白（按钮在上层，不会误触）。
    // 释放帧 `dragged()` 已为 false、只有 `drag_stopped()` 为 true：
    // 必须把它也纳入选择，否则会回退到未拖拽的胶囊响应而丢掉结束事件。
    let drag = if grip_resp.dragged() || grip_resp.drag_started() || grip_resp.drag_stopped() {
        grip_resp
    } else {
        pill_resp
    };
    if drag.drag_started() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ToolPanelDragStarted);
    } else if drag.drag_stopped() {
        // 释放帧 `dragged()` 仍为 true：必须先判结束，否则生产侧拖拽态永不收尾。
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ToolPanelDragEnded);
    } else if drag.dragged() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            toolbar_popup::push_action(
                ui.ctx(),
                ToolbarAction::ToolPanelDragged { x: pos.x, y: pos.y },
            );
        }
    } else if drag.drag_stopped() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ToolPanelDragEnded);
    }

    let mut blocked = vec![pill];
    if let Some(panel) = &model.panel {
        let owner = match panel.owner() {
            ToolPanelPanelOwner::Item(kind) => items
                .iter()
                .find(|(item, _)| *item == kind)
                .map(|(_, rect)| *rect),
            ToolPanelPanelOwner::Thickness => Some(thickness),
        };
        if let Some(owner) = owner {
            let rect = place_panel(area, owner, panel_height(panel));
            panels::show(ui, panel, rect, &mut blocked, icons);
        }
    }

    // 面板外点击 / Esc 关闭（点击胶囊按钮时其动作已占位，不会互相覆盖）。
    if let Some(panel) = &model.panel {
        let ctx = ui.ctx();
        if !toolbar_popup::action_pending(ctx) && clicks_outside(ui, &blocked) {
            toolbar_popup::push_action(ctx, close_action(panel));
        }
        if !toolbar_popup::action_pending(ctx) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            toolbar_popup::push_action(ctx, close_action(panel));
        }
    }

    super::menu::extend_blocked(ui.ctx(), blocked);

    set_triggers(
        ui.ctx(),
        Triggers {
            pill,
            area,
            items,
            thickness,
        },
    );
}

/// 指针是否在占用区外点击。
fn clicks_outside(ui: &egui::Ui, blocked: &[Rect]) -> bool {
    ui.input(|i| i.pointer.any_click())
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|pos| !blocked.iter().any(|rect| rect.contains(pos)))
}

/// 面板对应的关闭动作。
fn close_action(panel: &ToolPanelSettingsPanel) -> ToolbarAction {
    match panel {
        ToolPanelSettingsPanel::Brush(_) => ToolbarAction::CloseBrushPanel,
        ToolPanelSettingsPanel::Shape(_) => ToolbarAction::CloseShapePanel,
        ToolPanelSettingsPanel::FillDivision(_) => ToolbarAction::CloseFillPanel,
        ToolPanelSettingsPanel::TextParams(_) => ToolbarAction::CloseTextPanel,
        ToolPanelSettingsPanel::Thickness(_) => ToolbarAction::CloseThicknessPanel,
    }
}

/// 胶囊内图标按钮：选中 / 悬停 / 按下底色 + 居中图标。
fn item_cell(
    ui: &mut egui::Ui,
    icons: &mut IconCache,
    icon: crate::icons::Icon,
    rect: Rect,
    radius: f32,
    selected: bool,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> egui::Response {
    let response = ui.interact(rect, Id::unique(id), Sense::click());
    let background = if selected {
        Some(theme::SELECTED_BG)
    } else if response.is_pointer_button_down_on() {
        Some(egui::Color32::from_rgb(0x55, 0x5E, 0x8F))
    } else if response.hovered() {
        Some(egui::Color32::from_rgb(0x47, 0x4F, 0x7A))
    } else {
        None
    };
    if let Some(background) = background {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius as u8), background);
    }
    let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(ICON));
    icons.paint(ui.painter(), icon, icon_rect);
    response
}
