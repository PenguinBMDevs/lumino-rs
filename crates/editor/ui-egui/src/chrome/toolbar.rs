//! 工具栏：走带、工具、仪表盘、右侧显示开关。
//!
//! 结构对齐生产 iced `toolbar_view`：
//! - 背景 = `background.weakest`（PANEL_BG）；
//! - 每个分组是一个 50px 高、圆角 4 的 `background.weak`（TOOLBAR_BG）容器；
//! - 分组间距：录制 4 / 播放 8 / 循环 8 / 撤销 16 / 仪表盘 16 / 工具 0；
//! - 右侧 OnionSkin / AutoScroll / Collaboration / More 直接贴在工具栏底色上。
//!
//! 分组绘制拆到三个子模块（单文件行数纪律）：
//! - [`controls`]：录制 / 走带 / 循环 / 撤销重做 / 检测仪表盘；
//! - [`tools`]：工具选择区 + 精度下拉；
//! - [`right_side`]：洋葱皮 / 自动滚动 / 协作 / 「更多」。

mod controls;
mod right_side;
mod tools;

use egui::{Align, CornerRadius, Layout, Rect, Stroke, Vec2};
use lumino_core::Tool;

use super::{ChromeState, ToolbarModel};
use crate::icons::Icon;
use crate::layout;
use crate::theme;

/// 草稿场景入口：使用默认状态（与 iced 基线一致）。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    show_model(ui, &ToolbarModel::default(), &mut state.icons);
}

/// 渲染工具栏。
pub fn show_model(ui: &mut egui::Ui, model: &ToolbarModel, icons: &mut crate::icons::IconCache) {
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0, theme::panel_bg());
    if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
        tracing::warn!(?full, "DEBUG toolbar band");
    }

    let content = Rect::from_min_size(full.min, Vec2::new(full.width(), layout::TOOLBAR_CONTENT_H));
    let y = content.top() + layout::TOOLBAR_PAD_Y;
    let h = layout::TOOLBAR_GROUP_H;
    let mut x = content.left() + layout::TOOLBAR_PAD_X;

    // Record
    if model.groups.record {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::RECORD_GROUP_W, h)),
            |ui| controls::record(ui, model.recording),
        );
        x += layout::RECORD_GROUP_W + 4.0;
    }

    // Playback
    if model.groups.playback {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::PLAYBACK_GROUP_W, h)),
            |ui| controls::playback(ui, icons, model.playing),
        );
        x += layout::PLAYBACK_GROUP_W + 8.0;
    }

    // Loop
    if model.groups.looping {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::LOOP_GROUP_W, h)),
            |ui| controls::loop_button(ui, icons, model.looping),
        );
        x += layout::LOOP_GROUP_W + 8.0;
    }

    // Undo / Redo / Clock
    if model.groups.undo_redo {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::UNDO_REDO_GROUP_W, h)),
            |ui| controls::undo_redo(ui, icons),
        );
        x += layout::UNDO_REDO_GROUP_W + 16.0;
    }

    // Dashboard
    if model.groups.dashboard {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::DASHBOARD_GROUP_W, h)),
            |ui| controls::dashboard(ui, model),
        );
        x += layout::DASHBOARD_GROUP_W + 16.0;
    }

    // Tools（宽度由条目尺寸累加，对应 iced 的 `Length::Shrink`）
    if model.groups.tools {
        let visible = visible_tool_count(model);
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(tools::width(ui, visible), h)),
            |ui| tools::show(ui, icons, model),
        );
    }

    right_side::show(ui, content, icons, model);
}

/// 工具区可见条目数（`ALL` 前缀；模型默认 `usize::MAX` = 全部）。
fn visible_tool_count(model: &ToolbarModel) -> usize {
    model.visible_tools.min(tools::TOOLS.len())
}

/// 分组容器：先铺底色，再在其中布局。
pub(super) fn group<R>(ui: &mut egui::Ui, rect: Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(layout::TOOLBAR_GROUP_RADIUS),
        theme::toolbar_bg(),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing = Vec2::ZERO;
    add(&mut child)
}

/// 固定矩形内的居中图标按钮。
pub(super) fn icon_at(
    ui: &mut egui::Ui,
    cache: &mut crate::icons::IconCache,
    icon: Icon,
    rect: Rect,
    icon_size: f32,
    selected: bool,
) -> egui::Response {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
    );
    let image = cache.default_image(child.ctx(), icon, icon_size);
    let button = if selected {
        egui::Button::image(image)
            .fill(theme::selected_bg())
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(4))
    } else {
        egui::Button::image(image).frame(false)
    };
    child.add(button)
}

/// 当前工具是否对应工具区里的第 N 个选择器（0..=3）。
pub(super) fn tool_selected(model: &ToolbarModel, index: usize) -> bool {
    matches!(
        (index, model.tool),
        (0, Tool::Pointer) | (1, Tool::PointerYSelect) | (2, Tool::Pencil) | (3, Tool::Eraser)
    )
}
