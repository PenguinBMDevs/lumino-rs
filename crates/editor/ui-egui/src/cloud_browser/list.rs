//! 条目列表：可滚动容器与单行渲染。
//!
//! 行内分四种状态：重命名编辑、目录名（可点）、普通文件名、删除确认。
//! `entry_row` 只做分发，宽度与坐标仍按 上一代 的固定列口径计算。

use crate::chrome::{CloudBrowserModel, CloudEntryActionKind, CloudEntryModel};
use crate::text::text_width_ceil as text_width;
use crate::theme;
use egui::{Align, CornerRadius, RichText, Sense, Vec2};

use super::action::{CloudBrowserAction, push_action};
use super::layout::{BTN_GAP, INPUT_H, ROW_H, SIZE_W, TINY_H};
use super::widgets::{input, label_left, tiny_button};

/// 条目列表（可滚动，行距 4）。
pub(super) fn list(ui: &mut egui::Ui, rect: egui::Rect, model: &CloudBrowserModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
    );
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 8.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = false;
    }
    let visuals = child.visuals_mut();
    visuals.widgets.inactive.fg_stroke.color = theme::selected_bg();
    visuals.widgets.hovered.fg_stroke.color = theme::hover_border();
    visuals.widgets.active.fg_stroke.color = theme::active_bg();

    if let Some(placeholder) = &model.placeholder {
        let mut empty = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(
                    rect.min,
                    Vec2::new(rect.width(), 20.0),
                ))
                .layout(egui::Layout::left_to_right(Align::Center)),
        );
        empty.add(
            egui::Label::new(RichText::new(placeholder).size(12.0).color(theme::text()))
                .selectable(false)
                .truncate(),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("lumino.cloud-browser.list")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 2.5;
            for entry in &model.entries {
                entry_row(ui, entry, model);
            }
        });
}

/// 一行条目：按状态分发到重命名行或常规行。
fn entry_row(ui: &mut egui::Ui, entry: &CloudEntryModel, model: &CloudBrowserModel) {
    let width = ui.available_width().max(1.0);
    let row_h = if entry.renaming { INPUT_H } else { ROW_H };
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::hover());
    if entry.renaming {
        rename_row(ui, row, model, width, row_h);
    } else {
        normal_row(ui, row, entry, model, width, row_h);
    }
}

/// 重命名编辑态：输入框 + 确定 / 取消。
fn rename_row(
    ui: &mut egui::Ui,
    row: egui::Rect,
    model: &CloudBrowserModel,
    width: f32,
    row_h: f32,
) {
    let ctx = ui.ctx().clone();
    let confirm_w = text_width(ui, &model.confirm, 12.0) + 16.0;
    let cancel_w = text_width(ui, &model.cancel, 12.0) + 16.0;
    let input_w = (width - confirm_w - cancel_w - BTN_GAP * 2.0).max(60.0);
    let mut x = row.left();
    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(x, row.top()), Vec2::new(input_w, row_h)),
        "cloud-rename-input",
        &model.rename_input,
        &model.rename_hint,
        |text| push_action(&ctx, CloudBrowserAction::RenameInputChanged(text)),
    );
    x += input_w + BTN_GAP;
    if tiny_button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, row.top() + (row_h - TINY_H) / 2.0),
            Vec2::new(confirm_w, TINY_H),
        ),
        &model.confirm,
        true,
    )
    .clicked()
    {
        push_action(&ctx, CloudBrowserAction::RenameConfirm);
    }
    x += confirm_w + BTN_GAP;
    if tiny_button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, row.top() + (row_h - TINY_H) / 2.0),
            Vec2::new(cancel_w, TINY_H),
        ),
        &model.cancel,
        true,
    )
    .clicked()
    {
        push_action(&ctx, CloudBrowserAction::RenameCancel);
    }
}

/// 常规行：名称列 + 大小列 + 行尾操作。
fn normal_row(
    ui: &mut egui::Ui,
    row: egui::Rect,
    entry: &CloudEntryModel,
    model: &CloudBrowserModel,
    width: f32,
    row_h: f32,
) {
    let actions_w = actions_width(ui, entry, model);
    let name_w = (width - SIZE_W - actions_w - BTN_GAP * 2.0).max(60.0);
    let mut x = row.left();
    name_cell(ui, row, entry, x, name_w, row_h);
    x += name_w + BTN_GAP;

    if !entry.size.is_empty() {
        label_left(
            ui,
            egui::Rect::from_min_size(egui::pos2(x, row.top()), Vec2::new(SIZE_W, row_h)),
            &entry.size,
            12.0,
            theme::text(),
        );
    }
    x += SIZE_W + BTN_GAP;

    if entry.pending_delete {
        delete_confirm_row(ui, row, entry, model, x, row_h);
    } else {
        action_buttons(ui, row, entry, model, x, row_h);
    }
}

/// 行尾操作区宽度（删除确认态固定三个控件，否则按按钮文案累加）。
fn actions_width(ui: &mut egui::Ui, entry: &CloudEntryModel, model: &CloudBrowserModel) -> f32 {
    if entry.pending_delete {
        text_width(ui, &model.confirm_delete, 12.0)
            + 2.0 * (text_width(ui, &model.delete, 12.0) + 16.0)
            + text_width(ui, &model.cancel, 12.0)
            + 16.0
            + 3.0 * BTN_GAP
    } else {
        entry
            .actions
            .iter()
            .map(|kind| text_width(ui, kind_label(model, *kind), 12.0) + 16.0 + BTN_GAP)
            .sum()
    }
}

/// 名称列：目录可点进入，文件只显示（超宽省略）。
fn name_cell(
    ui: &mut egui::Ui,
    row: egui::Rect,
    entry: &CloudEntryModel,
    x: f32,
    name_w: f32,
    row_h: f32,
) {
    if entry.is_dir {
        let response = ui.interact(
            egui::Rect::from_min_size(egui::pos2(x, row.top()), Vec2::new(name_w, row_h)),
            egui::Id::unique(("cloud-entry", &entry.path)),
            Sense::click(),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &entry.name));
        if response.hovered() {
            ui.painter()
                .rect_filled(response.rect, CornerRadius::same(3), theme::toolbar_bg());
        }
        label_left(
            ui,
            egui::Rect::from_min_max(
                egui::pos2(x + 8.0, row.top()),
                egui::pos2(x + name_w - 8.0, row.bottom()),
            ),
            &entry.name,
            13.0,
            theme::text(),
        );
        if response.clicked() {
            push_action(ui.ctx(), CloudBrowserAction::EnterDir(entry.path.clone()));
        }
    } else {
        label_left(
            ui,
            egui::Rect::from_min_size(
                egui::pos2(x + 8.0, row.top()),
                Vec2::new(name_w - 8.0, row_h),
            ),
            &entry.name,
            13.0,
            theme::text(),
        );
    }
}

/// 删除确认态：提示文案 + 删除 / 取消。
fn delete_confirm_row(
    ui: &mut egui::Ui,
    row: egui::Rect,
    entry: &CloudEntryModel,
    model: &CloudBrowserModel,
    x: f32,
    row_h: f32,
) {
    let ctx = ui.ctx().clone();
    let mut x = x;
    label_left(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, row.top()),
            Vec2::new(text_width(ui, &model.confirm_delete, 12.0) + BTN_GAP, row_h),
        ),
        &model.confirm_delete,
        12.0,
        theme::text(),
    );
    x += text_width(ui, &model.confirm_delete, 12.0) + BTN_GAP;
    let delete_w = text_width(ui, &model.delete, 12.0) + 16.0;
    if tiny_button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, row.top() + (row_h - TINY_H) / 2.0),
            Vec2::new(delete_w, TINY_H),
        ),
        &model.delete,
        true,
    )
    .clicked()
    {
        push_action(
            &ctx,
            CloudBrowserAction::DeleteEntry {
                path: entry.path.clone(),
                is_dir: entry.is_dir,
            },
        );
    }
    x += delete_w + BTN_GAP;
    let cancel_w = text_width(ui, &model.cancel, 12.0) + 16.0;
    if tiny_button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, row.top() + (row_h - TINY_H) / 2.0),
            Vec2::new(cancel_w, TINY_H),
        ),
        &model.cancel,
        true,
    )
    .clicked()
    {
        push_action(&ctx, CloudBrowserAction::DeleteCancel);
    }
}

/// 常规行尾：按 `entry.actions` 顺序渲染小按钮。
fn action_buttons(
    ui: &mut egui::Ui,
    row: egui::Rect,
    entry: &CloudEntryModel,
    model: &CloudBrowserModel,
    x: f32,
    row_h: f32,
) {
    let mut x = x;
    for kind in &entry.actions {
        let label = kind_label(model, *kind);
        let w = text_width(ui, label, 12.0) + 16.0;
        if tiny_button(
            ui,
            egui::Rect::from_min_size(
                egui::pos2(x, row.top() + (row_h - TINY_H) / 2.0),
                Vec2::new(w, TINY_H),
            ),
            label,
            true,
        )
        .clicked()
        {
            push_action(ui.ctx(), entry_action(*kind, entry));
        }
        x += w + BTN_GAP;
    }
}

/// 条目按钮 → 动作。
fn entry_action(kind: CloudEntryActionKind, entry: &CloudEntryModel) -> CloudBrowserAction {
    match kind {
        CloudEntryActionKind::Download => CloudBrowserAction::Download(entry.path.clone()),
        CloudEntryActionKind::Copy => CloudBrowserAction::Copy {
            path: entry.path.clone(),
            is_dir: entry.is_dir,
        },
        CloudEntryActionKind::Cut => CloudBrowserAction::Cut {
            path: entry.path.clone(),
            is_dir: entry.is_dir,
        },
        CloudEntryActionKind::Rename => CloudBrowserAction::StartRename(entry.path.clone()),
        CloudEntryActionKind::Delete => CloudBrowserAction::RequestDelete {
            path: entry.path.clone(),
            is_dir: entry.is_dir,
        },
    }
}

/// 条目按钮文案（模型里的 `labels` 由生产侧本地化）。
fn kind_label(model: &CloudBrowserModel, kind: CloudEntryActionKind) -> &str {
    kind.label(&model.labels)
}
