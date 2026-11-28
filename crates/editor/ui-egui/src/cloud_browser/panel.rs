//! 浏览面板与断连提醒的整窗装配。
//!
//! 浏览面板自上而下：工具栏（设备下拉 + 刷新 / 断开 / 粘贴 / 清空剪贴板）、
//! 导航行（返回 + 当前路径）、新建文件夹行、可选的保存目标区、条目列表、
//! 底部提示。每一段单独一个函数，`show` 只负责次序与纵向游标。

use crate::chrome::{CloudBrowserModel, CloudNoticeModel, CloudSavePanelModel};
use crate::text::text_width_ceil as text_width;
use crate::theme;
use egui::{Align, CornerRadius, RichText, Stroke, Vec2};

use super::action::{CloudBrowserAction, push_action};
use super::layout::{BTN_H, GAP, INPUT_H, PAD};
use super::list::list;
use super::widgets::{button, input, label_center, label_left, small_button};

/// 渲染浏览面板（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &CloudBrowserModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let body = crate::layout::body_rect(rect, model.titlebar);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    let mut y = inner.top();
    y = toolbar(ui, inner, y, model);
    y = nav_row(ui, inner, y, model);
    y = new_folder_row(ui, inner, y, model);

    // 保存目标区（可选）。
    if let Some(panel) = &model.save_panel {
        y += save_panel(ui, inner, y, panel) + GAP;
    }

    // 列表区：占据剩余高度（底部预留提示行），提示行与列表同帧绘制。
    list_and_notice(ui, inner, y, model);
}

/// 工具栏：设备下拉（填满）+ 刷新 / 断开 / 粘贴 / 清空剪贴板。
fn toolbar(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &CloudBrowserModel) -> f32 {
    let ctx = ui.ctx().clone();
    let full = inner.width();
    let mut x = inner.left();
    let labels = [
        (&model.refresh, true, CloudBrowserAction::Refresh),
        (
            &model.disconnect,
            model.devices.iter().any(|d| d.selected),
            CloudBrowserAction::Disconnect,
        ),
        (&model.paste, model.clipboard, CloudBrowserAction::Paste),
        (
            &model.clear_clipboard,
            model.clipboard,
            CloudBrowserAction::ClearClipboard,
        ),
    ];
    let mut buttons_w = 0.0;
    for (label, _, _) in &labels {
        buttons_w += text_width(ui, label, 12.0) + 20.0 + GAP;
    }
    let device_w = (full - buttons_w).max(120.0);
    device_combo(
        ui,
        egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(device_w, BTN_H)),
        model,
    );
    x += device_w + GAP;
    for (label, enabled, action) in labels {
        let w = text_width(ui, label, 12.0) + 20.0;
        if small_button(
            ui,
            egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, BTN_H)),
            label,
            12.0,
            enabled,
        )
        .clicked()
            && enabled
        {
            push_action(&ctx, action);
        }
        x += w + GAP;
    }
    y + BTN_H + GAP
}

/// 导航行：返回 + 当前路径。
fn nav_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &CloudBrowserModel) -> f32 {
    let back_w = text_width(ui, &model.back, 12.0) + 20.0;
    if small_button(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(back_w, BTN_H)),
        &model.back,
        12.0,
        true,
    )
    .clicked()
    {
        push_action(ui.ctx(), CloudBrowserAction::Back);
    }
    let path = if model.current_path.is_empty() {
        "/"
    } else {
        model.current_path.as_str()
    };
    label_left(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(inner.left() + back_w + GAP, y),
            egui::pos2(inner.right(), y + BTN_H),
        ),
        path,
        13.0,
        theme::text(),
    );
    y + BTN_H + GAP
}

/// 新建文件夹行：输入框 + 新建按钮（按钮在输入框内垂直居中）。
fn new_folder_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &CloudBrowserModel) -> f32 {
    let ctx = ui.ctx().clone();
    let full = inner.width();
    let new_w = text_width(ui, &model.new_folder, 12.0) + 20.0;
    input(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.left(), y),
            Vec2::new(full - new_w - GAP, INPUT_H),
        ),
        "cloud-new-folder",
        &model.new_folder_input,
        &model.new_folder_hint,
        |text| push_action(&ctx, CloudBrowserAction::NewFolderInputChanged(text)),
    );
    if small_button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.right() - new_w, y + (INPUT_H - BTN_H) / 2.0),
            Vec2::new(new_w, BTN_H),
        ),
        &model.new_folder,
        12.0,
        true,
    )
    .clicked()
    {
        push_action(&ctx, CloudBrowserAction::NewFolder);
    }
    y + INPUT_H + GAP
}

/// 列表区 + 底部提示行（提示行高度从列表可用高度里预留）。
fn list_and_notice(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &CloudBrowserModel) {
    let notice_h = if model.notice.is_some() {
        18.0 + GAP
    } else {
        0.0
    };
    let list_bottom = (inner.bottom() - notice_h).max(y);
    list(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(inner.left(), y),
            egui::pos2(inner.right(), list_bottom),
        ),
        model,
    );

    if let Some(notice) = &model.notice {
        label_left(
            ui,
            egui::Rect::from_min_size(
                egui::pos2(inner.left(), inner.bottom() - 18.0),
                Vec2::new(inner.width(), 18.0),
            ),
            notice,
            12.0,
            theme::text(),
        );
    }
}

/// 设备下拉（`pick_list`：宽度填满、显示选中项）。
fn device_combo(ui: &mut egui::Ui, rect: egui::Rect, model: &CloudBrowserModel) {
    let selected = model
        .devices
        .iter()
        .find(|device| device.selected)
        .map(|device| device.label.as_str())
        .unwrap_or(&model.device_placeholder);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().interact_size.y = BTN_H;
    let mut chosen = None;
    egui::ComboBox::from_id_salt("cloud-device")
        .selected_text(RichText::new(selected).size(theme::font::LABEL))
        .width(rect.width() - 6.0)
        .truncate()
        .show_ui(&mut child, |ui| {
            for device in &model.devices {
                if ui
                    .selectable_label(device.selected, &device.label)
                    .clicked()
                {
                    chosen = Some(device.index);
                }
            }
        });
    if let Some(index) = chosen {
        push_action(ui.ctx(), CloudBrowserAction::SelectDevice(index));
    }
}

/// 保存目标区：weak 底 + 圆角 6 + 内边距 10；标题 13px + 8 + 按钮。
fn save_panel(ui: &mut egui::Ui, inner: egui::Rect, y: f32, panel: &CloudSavePanelModel) -> f32 {
    let height = 10.0 + 20.0 + 8.0 + 30.0 + 10.0;
    let rect = egui::Rect::from_min_size(
        egui::pos2(inner.left(), y),
        Vec2::new(inner.width(), height),
    );
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        theme::toolbar_bg(),
        Stroke::NONE,
        egui::StrokeKind::Inside,
    );
    label_left(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
            Vec2::new(rect.width() - 20.0, 20.0),
        ),
        &panel.title,
        13.0,
        theme::text(),
    );
    let w = text_width(ui, &panel.save, 14.0) + 40.0;
    if button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + 10.0, rect.top() + 10.0 + 20.0 + 8.0),
            Vec2::new(w, 30.0),
        ),
        &panel.save,
        14.0,
        true,
    )
    .clicked()
    {
        push_action(ui.ctx(), CloudBrowserAction::SaveHere);
    }
    let _ = panel.busy;
    height
}

/// 渲染断连提醒。
pub fn show_notice(ui: &mut egui::Ui, model: &CloudNoticeModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let body = crate::layout::body_rect(rect, model.titlebar);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(20.0);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    // 列居中：标题 16px、14 间隔、正文 13px、14 间隔、按钮。
    let mut y = inner.top();
    label_center(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 24.0)),
        &model.title,
        16.0,
        theme::text(),
    );
    y += 24.0 + 14.0;
    label_center(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 20.0)),
        &model.message,
        13.0,
        theme::text(),
    );
    y += 20.0 + 14.0;
    let w = text_width(ui, &model.dismiss, 13.0) + 40.0;
    if button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.center().x - w / 2.0, y),
            Vec2::new(w, 34.0),
        ),
        &model.dismiss,
        13.0,
        true,
    )
    .clicked()
    {
        push_action(ui.ctx(), CloudBrowserAction::DismissNotice);
    }
}
