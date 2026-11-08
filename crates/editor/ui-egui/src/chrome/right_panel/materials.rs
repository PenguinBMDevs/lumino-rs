//! 右侧栏「素材库」面板渲染。
//!
//! 结构对齐生产 `right_sidebar/materials_view.rs`：
//! - 内容内边距 8、列间距 8；标题 14px、小标题 12px；
//! - 「添加素材」全宽主色按钮（`primary.weak` 底 / 悬停 `primary.base`）；
//! - 素材条目 `background.weaker` 底、圆角 4、内边距 `[6,8]`，名称 12px；
//! - 右键上下文菜单 56×144（40px 图标按钮 ×3、圆角 8、近黑底），在鼠标
//!   右下方展开，贴近面板右/下缘时翻转到另一侧；
//! - 悬停提示显示名称 / 作者 / 位置 / 轨道数 / 来源。

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Layout, Margin, Pos2, Rect, Sense, Stroke, Vec2,
};

mod ctx;

use super::super::model::{MaterialItemModel, MaterialMenuKind, MaterialsPanelModel};
use super::widgets;
use super::{RightSidebarAction, push_action, record_rect};
use crate::icons::IconCache;
use crate::theme;

const PAD: f32 = 8.0;
const GAP: f32 = 8.0;
const HEADER_H: f32 = 20.0;
const BUTTON_H: f32 = 28.0;
const MENU_PAD: f32 = 4.0;
const MENU_GAP: f32 = 2.0;
const MENU_ITEM_H: f32 = 24.0;
const SECTION_H: f32 = 18.0;
const SECTION_GAP: f32 = 4.0;
const ITEM_H: f32 = 27.0;
const ITEM_GAP: f32 = 2.0;
const ITEM_PAD_X: f32 = 8.0;

fn content_height(model: &MaterialsPanelModel) -> f32 {
    let mut height = PAD * 2.0 + HEADER_H + GAP + BUTTON_H;
    if let Some(items) = &model.add_menu {
        height += MENU_GAP
            + MENU_PAD * 2.0
            + items.len() as f32 * MENU_ITEM_H
            + items.len().saturating_sub(1) as f32 * MENU_GAP;
    }
    height += GAP;
    if model.placeholder.is_some() {
        height += 18.0;
    }
    for section in &model.sections {
        height += SECTION_H + SECTION_GAP;
        height += section.entries.len() as f32 * (ITEM_H + ITEM_GAP);
        height += GAP;
    }
    height
}

/// 绘制面板（`rect` = 手柄右侧的内容区）。
pub(super) fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &MaterialsPanelModel,
    icons: &mut IconCache,
) {
    let total = content_height(model);
    widgets::scroll_body(
        ui,
        rect,
        "lumino.right-sidebar.materials",
        total,
        |ui, content| draw_panel(ui, content, model, icons),
    );
}

fn draw_panel(ui: &mut egui::Ui, rect: Rect, model: &MaterialsPanelModel, icons: &mut IconCache) {
    let left = rect.left() + PAD;
    let right = rect.right() - PAD;
    let width = (right - left).max(0.0);
    let mut y = rect.top() + PAD;

    // 标题。
    ui.painter().text(
        Pos2::new(left, y + HEADER_H / 2.0),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(theme::font::LABEL),
        theme::TEXT_ON_WEAK,
    );
    y += HEADER_H + GAP;

    // 添加素材按钮。
    let add = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, BUTTON_H));
    record_rect(ui.ctx(), "material:add".to_owned(), add);
    let response = ui.interact(add, Id::unique("material-add"), Sense::click());
    ui.painter().rect_filled(
        add,
        CornerRadius::same(4),
        if response.hovered() {
            theme::ACCENT
        } else {
            theme::ACCENT_DARK
        },
    );
    ui.painter().text(
        add.center(),
        Align2::CENTER_CENTER,
        &model.add_label,
        FontId::proportional(theme::font::BODY),
        theme::TEXT,
    );
    if response.clicked() {
        push_action(ui.ctx(), RightSidebarAction::MaterialAdd);
    }
    y += BUTTON_H;

    // 添加下拉菜单（贴着按钮、面板底色）。
    if let Some(items) = &model.add_menu {
        y += MENU_GAP;
        let panel_h = MENU_PAD * 2.0
            + items.len() as f32 * MENU_ITEM_H
            + items.len().saturating_sub(1) as f32 * MENU_GAP;
        let panel = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, panel_h));
        ui.painter()
            .rect_filled(panel, CornerRadius::same(4), theme::TOOLBAR_BG);
        let mut item_y = panel.top() + MENU_PAD;
        for entry in items {
            let inner = Rect::from_min_size(
                Pos2::new(left + MENU_PAD, item_y),
                Vec2::new(width - MENU_PAD * 2.0, MENU_ITEM_H),
            );
            let key = match entry.kind {
                MaterialMenuKind::DownloadFromWeb => "material:menu:download",
                MaterialMenuKind::ImportFromLocal => "material:menu:local",
            };
            record_rect(ui.ctx(), key.to_owned(), inner);
            let response = ui.interact(inner, Id::unique(key), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(inner, CornerRadius::same(4), theme::BASE_BG);
            }
            ui.painter().text(
                Pos2::new(inner.left() + 6.0, inner.center().y),
                Align2::LEFT_CENTER,
                &entry.label,
                FontId::proportional(12.0),
                theme::TEXT,
            );
            if response.clicked() {
                push_action(ui.ctx(), RightSidebarAction::MaterialMenu(entry.kind));
            }
            item_y += MENU_ITEM_H + MENU_GAP;
        }
        y += panel_h;
    }
    y += GAP;

    // 列表占位 / 分区。
    if let Some(placeholder) = &model.placeholder {
        ui.painter().text(
            Pos2::new(left, y + 9.0),
            Align2::LEFT_CENTER,
            placeholder,
            FontId::proportional(12.0),
            theme::TEXT_MUTED,
        );
        y += 18.0;
    }
    for section in &model.sections {
        ui.painter().text(
            Pos2::new(left, y + SECTION_H / 2.0),
            Align2::LEFT_CENTER,
            &section.label,
            FontId::proportional(12.0),
            theme::TEXT_MUTED,
        );
        y += SECTION_H + SECTION_GAP;
        for item in &section.entries {
            draw_item(
                ui,
                rect,
                left,
                y,
                width,
                item,
                model.context_menu.is_none(),
                &model.rename_placeholder,
            );
            y += ITEM_H + ITEM_GAP;
        }
        y += GAP;
    }

    if let Some(menu) = &model.context_menu {
        ctx::draw(ui, rect, menu, model, icons);
    }
}

/// 单个素材条目。
#[allow(clippy::too_many_arguments)]
fn draw_item(
    ui: &mut egui::Ui,
    content: Rect,
    left: f32,
    y: f32,
    width: f32,
    item: &MaterialItemModel,
    menu_closed: bool,
    rename_placeholder: &str,
) {
    let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, ITEM_H));
    record_rect(ui.ctx(), format!("material:item:{}", item.index), rect);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), theme::ROUTE_BAR_BG);

    if let Some(buffer) = &item.renaming {
        let input_rect = rect.shrink2(Vec2::new(ITEM_PAD_X, 3.0));
        record_rect(
            ui.ctx(),
            format!("material:rename:{}", item.index),
            input_rect,
        );
        ui.painter().rect(
            input_rect,
            CornerRadius::same(2),
            theme::BASE_BG,
            Stroke::new(1.0, theme::SELECTED_BG),
            egui::StrokeKind::Inside,
        );
        let mut text = buffer.clone();
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(input_rect.shrink2(Vec2::new(4.0, 0.0)))
                .layout(Layout::left_to_right(egui::Align::Center)),
        );
        let response = child
            .push_id(("material-rename", item.index), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .frame(egui::Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(FontId::proportional(12.0))
                        .text_color(theme::TEXT)
                        .hint_text(
                            egui::RichText::new(rename_placeholder)
                                .size(12.0)
                                .color(theme::TEXT_DIM),
                        )
                        .desired_width(f32::INFINITY),
                )
            })
            .inner;
        if response.changed() {
            push_action(ui.ctx(), RightSidebarAction::MaterialRenameInput(text));
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            push_action(ui.ctx(), RightSidebarAction::MaterialRenameConfirmed);
        }
        return;
    }

    let color = if item.valid {
        theme::TEXT_ON_WEAK
    } else {
        theme::TEXT_DIM
    };
    ui.painter().text(
        Pos2::new(left + ITEM_PAD_X, rect.center().y),
        Align2::LEFT_CENTER,
        &item.name,
        FontId::proportional(12.0),
        color,
    );

    let hover = ui.interact(
        rect,
        Id::unique(("material-item", item.index)),
        Sense::hover(),
    );
    if !item.tooltip.is_empty() {
        hover.on_hover_ui(|ui| {
            ui.set_max_width(280.0);
            for line in &item.tooltip {
                ui.label(
                    egui::RichText::new(line)
                        .size(10.0)
                        .color(Color32::from_rgb(0xF2, 0xF2, 0xF2)),
                );
            }
        });
    }

    // 有效素材：按下即开始拖出；右键打开上下文菜单。无效素材不可交互。
    if !item.valid || !menu_closed {
        return;
    }
    if widgets::pressed_in(ui, rect, egui::PointerButton::Primary) {
        push_action(
            ui.ctx(),
            RightSidebarAction::MaterialDragStarted(item.index),
        );
    } else if widgets::pressed_in(ui, rect, egui::PointerButton::Secondary)
        && let Some(pointer) = widgets::pointer_pos(ui)
    {
        push_action(
            ui.ctx(),
            RightSidebarAction::MaterialContextMenuOpened {
                index: item.index,
                x: pointer.x - content.left(),
                y: pointer.y - content.top(),
            },
        );
    }
}
