//! 工具栏：走带、工具、仪表盘、右侧显示开关。
//!
//! 结构对齐生产 iced `toolbar_view`：
//! - 背景 = `background.weakest`（PANEL_BG）；
//! - 每个分组是一个 50px 高、圆角 4 的 `background.weak`（TOOLBAR_BG）容器；
//! - 分组间距：录制 4 / 播放 8 / 循环 8 / 撤销 16 / 仪表盘 16 / 工具 0；
//! - 右侧 OnionSkin / AutoScroll / More 直接贴在工具栏底色上。

mod tools;

use egui::{Align, Align2, CornerRadius, FontId, Layout, Rect, Sense, Stroke, Vec2};
use lumino_core::Tool;
use lumino_core::storage::config::AutoScrollMode;

use super::{ChromeState, ToolbarModel};
use crate::icons::Icon;
use crate::layout;
use crate::theme;
use crate::widgets;

/// 草稿场景入口：使用默认状态（与 iced 基线一致）。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    show_model(ui, &ToolbarModel::default(), &mut state.icons);
}

/// 渲染工具栏。
pub fn show_model(ui: &mut egui::Ui, model: &ToolbarModel, icons: &mut crate::icons::IconCache) {
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0, theme::PANEL_BG);

    let content = Rect::from_min_size(full.min, Vec2::new(full.width(), layout::TOOLBAR_CONTENT_H));
    let y = content.top() + layout::TOOLBAR_PAD_Y;
    let h = layout::TOOLBAR_GROUP_H;
    let mut x = content.left() + layout::TOOLBAR_PAD_X;

    // Record
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::RECORD_GROUP_W, h)),
        |ui| record(ui, model.recording),
    );
    x += layout::RECORD_GROUP_W + 4.0;

    // Playback
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::PLAYBACK_GROUP_W, h)),
        |ui| playback(ui, icons, model.playing),
    );
    x += layout::PLAYBACK_GROUP_W + 8.0;

    // Loop
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::LOOP_GROUP_W, h)),
        |ui| {
            ui.add_space((layout::LOOP_GROUP_W - layout::TOOL_BUTTON_W) / 2.0);
            let icon = if model.looping {
                Icon::ArrowsLeftRight
            } else {
                Icon::Ban
            };
            widgets::icon_button(
                ui,
                icons,
                icon,
                layout::TOOL_BUTTON_ICON,
                Vec2::splat(layout::TOOL_BUTTON_W),
                false,
            );
        },
    );
    x += layout::LOOP_GROUP_W + 8.0;

    // Undo / Redo / Clock
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::UNDO_REDO_GROUP_W, h)),
        |ui| {
            let total = layout::TOOL_BUTTON_W * 3.0;
            ui.add_space((layout::UNDO_REDO_GROUP_W - total) / 2.0);
            for icon in [Icon::Undo, Icon::Redo, Icon::Clock] {
                widgets::icon_button(
                    ui,
                    icons,
                    icon,
                    layout::TOOL_BUTTON_ICON,
                    Vec2::splat(layout::TOOL_BUTTON_W),
                    false,
                );
            }
        },
    );
    x += layout::UNDO_REDO_GROUP_W + 16.0;

    // Dashboard
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::DASHBOARD_GROUP_W, h)),
        |ui| dashboard(ui, model),
    );
    x += layout::DASHBOARD_GROUP_W + 16.0;

    // Tools（宽度由条目尺寸累加，对应 iced 的 `Length::Shrink`）
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(tools::width(ui), h)),
        |ui| tools::show(ui, icons, model),
    );

    right_side(ui, content, icons, model);
}

/// 右侧显示开关：OnionSkin(+caret) / AutoScroll / More。
fn right_side(
    ui: &mut egui::Ui,
    content: Rect,
    icons: &mut crate::icons::IconCache,
    model: &ToolbarModel,
) {
    let more_right = content.right() - layout::TOOLBAR_PAD_X;
    let more = Rect::from_min_size(
        egui::pos2(more_right - layout::MORE_BUTTON_W, content.top() + 19.0),
        Vec2::new(layout::MORE_BUTTON_W, layout::RIGHT_TOOL_W),
    );
    let scroll_rect = Rect::from_min_size(
        egui::pos2(
            more.left() - 36.0 - layout::RIGHT_TOOL_W,
            content.top() + 17.0,
        ),
        Vec2::splat(layout::RIGHT_TOOL_W),
    );
    let caret_rect = Rect::from_min_size(
        egui::pos2(scroll_rect.left() - 32.0 - 16.0, content.top() + 17.0),
        Vec2::new(16.0, layout::RIGHT_TOOL_W),
    );
    let onion_rect = Rect::from_min_size(
        egui::pos2(
            caret_rect.left() - layout::RIGHT_TOOL_W,
            content.top() + 17.0,
        ),
        Vec2::splat(layout::RIGHT_TOOL_W),
    );

    // 洋葱皮开启：`background.strong` 底色（与工具选中态同源）。
    if model.onion {
        ui.painter()
            .rect_filled(onion_rect, CornerRadius::same(4), theme::SELECTED_BG);
    }
    let onion = icon_at(ui, icons, Icon::OnionSkin, onion_rect, 20.0, model.onion);
    let caret = icon_at(ui, icons, Icon::CaretDown, caret_rect, 12.0, false);
    super::toolbar_popup::set_onion_triggers(ui.ctx(), onion_rect, caret_rect);
    if onion.clicked() {
        super::toolbar_popup::push_action(
            ui.ctx(),
            super::toolbar_popup::ToolbarAction::ToggleOnionSkin,
        );
    }
    if caret.clicked() {
        super::toolbar_popup::push_action(
            ui.ctx(),
            super::toolbar_popup::ToolbarAction::ToggleOnionMenu,
        );
    }
    let scroll_icon = match model.auto_scroll {
        AutoScrollMode::FixedIndicatorLeft => Icon::ArrowsLeftRight,
        AutoScrollMode::ScrollingIndicator => Icon::Scroll,
        AutoScrollMode::Off => Icon::Ban,
    };
    icon_at(ui, icons, scroll_icon, scroll_rect, 20.0, false);
    icon_at(ui, icons, Icon::EllipsisVertical, more, 18.0, false);
}

/// 分组容器：先铺底色，再在其中布局。
fn group<R>(ui: &mut egui::Ui, rect: Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(layout::TOOLBAR_GROUP_RADIUS),
        theme::TOOLBAR_BG,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing = Vec2::ZERO;
    add(&mut child)
}

fn record(ui: &mut egui::Ui, recording: bool) {
    let rect = ui.max_rect();
    let (_, response) = ui.allocate_exact_size(rect.size(), Sense::click());
    let _ = response;
    // iced 录制红点直径 8（基线实测 8×7）；录制中保持红点（图标语义由状态栏承担）。
    let _ = recording;
    ui.painter()
        .circle_filled(rect.center(), 4.0, theme::RECORD_RED);
}

fn playback(ui: &mut egui::Ui, cache: &mut crate::icons::IconCache, playing: bool) {
    let total = layout::TOOL_BUTTON_W * 3.0 + 8.0;
    ui.add_space((layout::PLAYBACK_GROUP_W - total) / 2.0);
    let center_icon = if playing { Icon::Pause } else { Icon::Play };
    for (index, icon) in [Icon::SkipBackward, center_icon, Icon::SkipForward]
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            ui.add_space(4.0);
        }
        widgets::icon_button(
            ui,
            cache,
            icon,
            layout::TOOL_BUTTON_ICON,
            Vec2::splat(layout::TOOL_BUTTON_W),
            false,
        );
    }
}

fn dashboard(ui: &mut egui::Ui, model: &ToolbarModel) {
    let rect = ui.max_rect();
    let split = rect.left() + layout::DASHBOARD_CELL_W;
    ui.painter().rect_filled(
        Rect::from_min_size(
            egui::pos2(split, rect.top() + 8.0),
            Vec2::new(1.0, rect.height() - 16.0),
        ),
        0,
        theme::BORDER,
    );

    metric(
        ui,
        Rect::from_min_max(rect.min, egui::pos2(split, rect.bottom())),
        &model.bpm,
        &model.ppq,
    );
    metric(
        ui,
        Rect::from_min_max(egui::pos2(split, rect.top()), rect.max),
        &model.position,
        &model.time,
    );
}

fn metric(ui: &egui::Ui, rect: Rect, value: &str, sub: &str) {
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 15.0),
        Align2::CENTER_CENTER,
        value,
        FontId::proportional(theme::font::BODY),
        theme::ACCENT,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 31.0),
        Align2::CENTER_CENTER,
        sub,
        FontId::proportional(theme::font::SMALL),
        theme::TEXT_MUTED,
    );
}

/// 固定矩形内的居中图标按钮。
fn icon_at(
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
            .fill(theme::SELECTED_BG)
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
