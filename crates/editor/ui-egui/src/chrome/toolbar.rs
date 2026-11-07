//! 工具栏：走带、工具、仪表盘、右侧显示开关。
//!
//! 结构对齐生产 iced `toolbar_view`：
//! - 背景 = `background.weakest`（PANEL_BG）；
//! - 每个分组是一个 50px 高、圆角 4 的 `background.weak`（TOOLBAR_BG）容器；
//! - 分组间距：录制 4 / 播放 8 / 循环 8 / 撤销 16 / 仪表盘 16 / 工具 0；
//! - 右侧 OnionSkin / AutoScroll / More 直接贴在工具栏底色上。

mod tools;

use egui::{Align, Align2, CornerRadius, FontId, Layout, Rect, Sense, Stroke, Vec2};

use super::ChromeState;
use crate::icons::Icon;
use crate::layout;
use crate::theme;
use crate::widgets;

/// 渲染工具栏。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
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
        record,
    );
    x += layout::RECORD_GROUP_W + 4.0;

    // Playback
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::PLAYBACK_GROUP_W, h)),
        |ui| playback(ui, &mut state.icons),
    );
    x += layout::PLAYBACK_GROUP_W + 8.0;

    // Loop
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::LOOP_GROUP_W, h)),
        |ui| {
            ui.add_space((layout::LOOP_GROUP_W - layout::TOOL_BUTTON_W) / 2.0);
            widgets::icon_button(
                ui,
                &mut state.icons,
                Icon::Ban,
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
                    &mut state.icons,
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
        dashboard,
    );
    x += layout::DASHBOARD_GROUP_W + 16.0;

    // Tools（宽度由条目尺寸累加，对应 iced 的 `Length::Shrink`）
    group(
        ui,
        Rect::from_min_size(egui::pos2(x, y), Vec2::new(tools::width(), h)),
        |ui| tools::show(ui, &mut state.icons),
    );

    right_side(ui, content, state);
}

/// 右侧显示开关：OnionSkin(+caret) / AutoScroll / More。
fn right_side(ui: &mut egui::Ui, content: Rect, state: &mut ChromeState) {
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

    icon_at(
        ui,
        &mut state.icons,
        Icon::OnionSkin,
        onion_rect,
        20.0,
        false,
    );
    icon_at(
        ui,
        &mut state.icons,
        Icon::CaretDown,
        caret_rect,
        12.0,
        false,
    );
    icon_at(ui, &mut state.icons, Icon::Scroll, scroll_rect, 20.0, false);
    icon_at(
        ui,
        &mut state.icons,
        Icon::EllipsisVertical,
        more,
        18.0,
        false,
    );
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

fn record(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let (_, response) = ui.allocate_exact_size(rect.size(), Sense::click());
    let _ = response;
    ui.painter()
        .circle_filled(rect.center(), 7.0, theme::RECORD_RED);
}

fn playback(ui: &mut egui::Ui, cache: &mut crate::icons::IconCache) {
    let total = layout::TOOL_BUTTON_W * 3.0 + 8.0;
    ui.add_space((layout::PLAYBACK_GROUP_W - total) / 2.0);
    for (index, icon) in [Icon::SkipBackward, Icon::Play, Icon::SkipForward]
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

fn dashboard(ui: &mut egui::Ui) {
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
        "120.0",
        "PPQ 1920",
    );
    metric(
        ui,
        Rect::from_min_max(egui::pos2(split, rect.top()), rect.max),
        "1.1",
        "00:00.00",
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
) {
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
    child.add(button);
}
