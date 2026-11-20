//! 悬浮工具条的触发矩形注册表（从 `tool_panel.rs` 拆出，保持单文件 < 400 行）。
//!
//! 只回答「本帧胶囊 / 各按钮 / 填充输入框在哪里」；绘制与动作仍在父模块。

use egui::{Id, Pos2, Rect, Vec2};

use super::panels::panel_height;
use super::{MENU_W, ToolPanelItemKind, ToolPanelPanelOwner, ToolPanelSettingsPanel};

const TRIGGERS_ID: &str = "lumino.tool-panel.triggers";
const FILL_INPUT_ID: &str = "lumino.tool-panel.fill-input";

/// 本帧记录的触发矩形（胶囊 + 各按钮 + 卷帘可用区）。
#[derive(Debug, Clone)]
pub(super) struct Triggers {
    /// 胶囊矩形（占用）。
    pub(super) pill: Rect,
    /// 卷帘区（面板定位与越界吸附用）。
    pub(super) area: Rect,
    /// 六个工具条目的矩形。
    pub(super) items: Vec<(ToolPanelItemKind, Rect)>,
    /// 右端「粗细」按钮矩形。
    pub(super) thickness: Rect,
}

/// 记录本帧胶囊与按钮矩形。
pub(crate) fn set_triggers(ctx: &egui::Context, triggers: Triggers) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(TRIGGERS_ID), Some(triggers)));
}

/// 工具条未渲染时清掉矩形（避免陈旧占用吞掉下层点击）。
pub(crate) fn clear_triggers(ctx: &egui::Context) {
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

pub(super) fn place_panel(area: Rect, owner: Rect, height: f32) -> Rect {
    let x = (owner.center().x - MENU_W / 2.0)
        .clamp(area.left(), (area.right() - MENU_W).max(area.left()));
    let mut y = owner.top() - height - 2.0;
    if y < area.top() {
        y = owner.bottom() + 2.0;
    }
    Rect::from_min_size(Pos2::new(x, y), Vec2::new(MENU_W, height))
}
