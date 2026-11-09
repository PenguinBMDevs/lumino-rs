//! 云文件浏览面板（`DialogType::CloudBrowser`）与断连提醒（`CloudNotice`）。
//!
//! 浏览面板：整窗 base 底 + 内边距 16，自上而下工具栏（设备下拉 + 刷新 / 断开 /
//! 粘贴 / 清空剪贴板）、导航行（返回 + 当前路径）、新建文件夹行、可选的保存目标区
//! （`background.weak` 底 + 圆角 6 + 内边距 10）、可滚动条目列表（行距 4）、
//! 12px 操作提示。条目行 = 名称（可点进目录）+ 80 宽大小 + 行尾小按钮。
//!
//! 动作只回传意图（下标 / 路径 / 字段文本），磁盘操作仍由 Runner 完成。

use egui::{Align, Color32, CornerRadius, FontId, RichText, Sense, Stroke, Vec2};

use crate::chrome::{
    CloudBrowserModel, CloudEntryActionKind, CloudEntryModel, CloudNoticeModel, CloudSavePanelModel,
};
use crate::theme;

const ACTION_ID: &str = "lumino.cloud-browser.action";
/// 面板内边距（生产 `container(...).padding(16)`）。
const PAD: f32 = 16.0;
/// 行间距（生产 `column.spacing(8)`）。
const GAP: f32 = 8.0;
/// 小按钮高度（12px 文本 + `padding([4,10])`）。
const BTN_H: f32 = 26.0;
/// 行尾小按钮（12px + `padding([2,8])`）。
const TINY_H: f32 = 22.0;
/// 列表行高（13px 名称 + `padding([3,8])`，实测 iced 行距 27 逻辑）。
const ROW_H: f32 = 23.0;
/// 输入框高度（`text_input` 默认内边距 + 13px 文本）。
const INPUT_H: f32 = 31.5;
/// 大小列宽（生产 `text.width(Fixed(80))`）。
const SIZE_W: f32 = 80.0;
/// 行尾按钮间距（生产 `row.spacing(6)`）。
const BTN_GAP: f32 = 6.0;

/// 云浏览面板产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudBrowserAction {
    /// 切换设备（`devices` 下标）。
    SelectDevice(usize),
    /// 刷新当前目录。
    Refresh,
    /// 断开当前设备。
    Disconnect,
    /// 粘贴剪贴板内容。
    Paste,
    /// 清空剪贴板。
    ClearClipboard,
    /// 返回上级目录。
    Back,
    /// 进入目录。
    EnterDir(String),
    /// 新建文件夹输入变化。
    NewFolderInputChanged(String),
    /// 新建文件夹。
    NewFolder,
    /// 保存到当前目录。
    SaveHere,
    /// 下载文件。
    Download(String),
    /// 复制条目。
    Copy {
        /// 远程路径。
        path: String,
        /// 是否为目录。
        is_dir: bool,
    },
    /// 剪切条目。
    Cut {
        /// 远程路径。
        path: String,
        /// 是否为目录。
        is_dir: bool,
    },
    /// 开始重命名。
    StartRename(String),
    /// 请求删除（进入确认态）。
    RequestDelete {
        /// 远程路径。
        path: String,
        /// 是否为目录。
        is_dir: bool,
    },
    /// 重命名输入变化。
    RenameInputChanged(String),
    /// 确认重命名。
    RenameConfirm,
    /// 取消重命名。
    RenameCancel,
    /// 确认删除。
    DeleteEntry {
        /// 远程路径。
        path: String,
        /// 是否为目录。
        is_dir: bool,
    },
    /// 取消删除。
    DeleteCancel,
    /// 关闭断连提醒。
    DismissNotice,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: CloudBrowserAction) {
    if ctx
        .data(|d| d.get_temp::<Option<CloudBrowserAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<CloudBrowserAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<CloudBrowserAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染浏览面板（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &CloudBrowserModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }
    let ctx = ui.ctx().clone();
    let full = inner.width();
    let mut y = inner.top();

    // 工具栏：设备下拉（填满）+ 刷新 / 断开 / 粘贴 / 清空剪贴板。
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
    y += BTN_H + GAP;

    // 导航行：返回 + 当前路径。
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
        push_action(&ctx, CloudBrowserAction::Back);
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
    y += BTN_H + GAP;

    // 新建文件夹行。
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
    y += INPUT_H + GAP;

    // 保存目标区（可选）。
    if let Some(panel) = &model.save_panel {
        let h = save_panel(ui, inner, y, panel);
        y += h + GAP;
    }

    // 列表区：占据剩余高度（底部预留提示行）。
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
                Vec2::new(full, 18.0),
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

/// 条目列表（可滚动，行距 4）。
fn list(ui: &mut egui::Ui, rect: egui::Rect, model: &CloudBrowserModel) {
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
        empty.add(egui::Label::new(
            RichText::new(placeholder).size(12.0).color(theme::text()),
        ));
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

/// 一行条目。
fn entry_row(ui: &mut egui::Ui, entry: &CloudEntryModel, model: &CloudBrowserModel) {
    let ctx = ui.ctx().clone();
    let width = ui.available_width().max(1.0);
    let row_h = if entry.renaming { INPUT_H } else { ROW_H };
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::hover());
    let mut x = row.left();

    // 名称区：重命名输入 + 确定 / 取消；否则名称按钮（目录可点）。
    if entry.renaming {
        let confirm_w = text_width(ui, &model.confirm, 12.0) + 16.0;
        let cancel_w = text_width(ui, &model.cancel, 12.0) + 16.0;
        let input_w = (width - confirm_w - cancel_w - BTN_GAP * 2.0).max(60.0);
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
        return;
    }

    // 行尾操作区宽度先算出来，名称按钮填满剩余。
    let actions_w = if entry.pending_delete {
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
            .sum::<f32>()
    };
    let name_w = (width - SIZE_W - actions_w - BTN_GAP * 2.0).max(60.0);
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
            push_action(&ctx, CloudBrowserAction::EnterDir(entry.path.clone()));
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

    // 行尾：删除确认态 / 常规操作按钮。
    if entry.pending_delete {
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
    } else {
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
                push_action(&ctx, entry_action(*kind, entry));
            }
            x += w + BTN_GAP;
        }
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

/// 渲染断连提醒。
pub fn show_notice(ui: &mut egui::Ui, model: &CloudNoticeModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
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

/// 主色按钮（`padding([6,20])`）。
fn button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    size: f32,
    primary: bool,
) -> egui::Response {
    let (fill, text) = if primary {
        (theme::accent(), theme::on_accent())
    } else {
        (theme::toolbar_bg(), theme::text())
    };
    let button = egui::Button::new(RichText::new(label).size(size).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

/// 工具栏按钮：iced 默认 `button` 是**主色底**；禁用态用 `primary.weak` 底。
fn small_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    size: f32,
    enabled: bool,
) -> egui::Response {
    // iced 的禁用态 = 主色 `scale_alpha(0.5)` 叠在底色上（实测 ≈ 0.33 不透明）。
    let (fill, text) = if enabled {
        (theme::accent(), theme::on_accent())
    } else {
        let fill = mix(theme::base_bg(), theme::accent(), 0.285);
        (fill, mix(fill, Color32::WHITE, 0.55))
    };
    let button = egui::Button::new(RichText::new(label).size(size).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

/// 行尾小按钮（12px、`padding([2,8])`、无底色）。
fn tiny_button(ui: &mut egui::Ui, rect: egui::Rect, label: &str, enabled: bool) -> egui::Response {
    let text = if enabled {
        theme::text()
    } else {
        theme::text_muted()
    };
    let button = egui::Button::new(RichText::new(label).size(12.0).color(text))
        .frame(false)
        .corner_radius(CornerRadius::same(3));
    ui.put(rect, button)
}

/// 固定盒子输入框。
fn input(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &'static str,
    value: &str,
    placeholder: &str,
    changed: impl FnOnce(String),
) {
    let mut buffer = value.to_owned();
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(8.0, 5.0)))
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("cloud-input", id), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::proportional(theme::font::LABEL))
                    .text_color(theme::text())
                    .hint_text(
                        RichText::new(placeholder)
                            .size(theme::font::LABEL)
                            .color(theme::text_dim()),
                    )
                    .desired_width(rect.width() - 18.0),
            )
        })
        .inner;
    if response.changed() {
        changed(buffer);
    }
}

/// 两色按 `t` 线性混合（`t = 0` → `a`，`t = 1` → `b`）。
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::text())
        .size()
        .x
        .ceil()
}

/// 固定矩形内的左对齐文本（保留可访问树节点）。
fn label_left(ui: &mut egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}

/// 居中文本（进入可访问树）。
fn label_center(ui: &mut egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    ));
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
