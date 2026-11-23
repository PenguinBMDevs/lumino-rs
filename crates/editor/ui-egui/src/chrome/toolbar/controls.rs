//! 工具栏左侧分组：录制 / 走带 / 循环 / 撤销重做 / 检测仪表盘。
//!
//! 这里只负责「画 + 命中」，点击一律经 `toolbar_popup::push_action` 回传；
//! 文案与状态由 [`ToolbarModel`] 提供，本 crate 不引 i18n。

use egui::{Align2, FontId, Rect, Sense, Vec2};

use super::super::ToolbarModel;
use super::super::toolbar_popup::{self, ToolbarAction};
use super::{
    DashboardBlur, DashboardField, MetricBlur, metric_blur_reported_id, metric_focus_once_id,
    push_dashboard_blur, reset_metric_edit_session,
};
use crate::icons::Icon;
use crate::layout;
use crate::theme;
use crate::widgets::{self, HoverTip};

/// 录制组：红点按钮（录制中填实心背景）。
pub(super) fn record(ui: &mut egui::Ui, recording: bool, tip: HoverTip) {
    let rect = ui.max_rect();
    let (_, response) = ui.allocate_exact_size(rect.size(), Sense::click());
    if recording {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), theme::record_red());
    }
    // 上一代：红点直径 8（基线实测 8×7），录制中文字反色。
    ui.painter().circle_filled(
        rect.center(),
        4.0,
        if recording {
            egui::Color32::WHITE
        } else {
            theme::record_red()
        },
    );
    let response = widgets::hover_tip(response, tip);
    if response.clicked() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::RecordToggled);
    }
}

/// 走带组：倒带 / 播放暂停 / 快进（图标 16，按钮 34×38，间距 4）。
///
/// 倒带 / 快进单点为「一个小节」：快进严格跳到下一条小节线；快退在本小节
/// 前 1/2 退到上一个小节，后 1/2 回本小节开头。按住 Ctrl 点击分别跳到
/// 全曲开头 / 末尾（全局 EOT）。修饰键在点击时读取，与工具区 Ctrl 命令同口径。
pub(super) fn playback(
    ui: &mut egui::Ui,
    cache: &mut crate::icons::IconCache,
    playing: bool,
    tips: &super::super::ToolbarTips,
) {
    let total = layout::TOOL_BUTTON_W * 3.0 + 8.0;
    ui.add_space((layout::PLAYBACK_GROUP_W - total) / 2.0);
    let center = if playing {
        (Icon::Pause, ToolbarAction::Pause, tips.play_pause)
    } else {
        (Icon::Play, ToolbarAction::Play, tips.play_pause)
    };
    let ctrl = ui.input(|i| i.modifiers.ctrl);
    let rewind = if ctrl {
        ToolbarAction::SkipBackwardToStart
    } else {
        ToolbarAction::SkipBackward
    };
    let forward = if ctrl {
        ToolbarAction::SkipForwardToEnd
    } else {
        ToolbarAction::SkipForward
    };
    for (index, (icon, action, tip)) in [
        (Icon::SkipBackward, rewind, tips.skip_backward),
        center,
        (Icon::SkipForward, forward, tips.skip_forward),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            ui.add_space(4.0);
        }
        let response = widgets::icon_button(
            ui,
            cache,
            icon,
            layout::TOOL_BUTTON_ICON,
            Vec2::splat(layout::TOOL_BUTTON_W),
            false,
        );
        let response = widgets::hover_tip(response, tip);
        if response.clicked() {
            toolbar_popup::push_action(ui.ctx(), action);
        }
    }
}

/// 循环按钮（开启时图标由 `Ban` 换成双向箭头，与 上一代 一致）。
pub(super) fn loop_button(
    ui: &mut egui::Ui,
    cache: &mut crate::icons::IconCache,
    looping: bool,
    tip: HoverTip,
) {
    ui.add_space((layout::LOOP_GROUP_W - layout::TOOL_BUTTON_W) / 2.0);
    let icon = if looping {
        Icon::ArrowsLeftRight
    } else {
        Icon::Ban
    };
    let response = widgets::icon_button(
        ui,
        cache,
        icon,
        layout::TOOL_BUTTON_ICON,
        Vec2::splat(layout::TOOL_BUTTON_W),
        false,
    );
    let response = widgets::hover_tip(response, tip);
    if response.clicked() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ToggleLoop);
    }
}

/// 撤销 / 重做 / 历史（96 宽组，三个 34 宽按钮）。
pub(super) fn undo_redo(
    ui: &mut egui::Ui,
    cache: &mut crate::icons::IconCache,
    tips: &super::super::ToolbarTips,
) {
    let total = layout::TOOL_BUTTON_W * 3.0;
    ui.add_space((layout::UNDO_REDO_GROUP_W - total) / 2.0);
    for (icon, action, tip) in [
        (Icon::Undo, ToolbarAction::Undo, tips.undo),
        (Icon::Redo, ToolbarAction::Redo, tips.redo),
        (Icon::Clock, ToolbarAction::OpenHistory, tips.history),
    ] {
        let response = widgets::icon_button(
            ui,
            cache,
            icon,
            layout::TOOL_BUTTON_ICON,
            Vec2::splat(layout::TOOL_BUTTON_W),
            false,
        );
        let response = widgets::hover_tip(response, tip);
        if response.clicked() {
            toolbar_popup::push_action(ui.ctx(), action);
        }
    }
}

/// 检测仪表盘：`BPM / PPQ` 与 `位置 / 时间` 两列，中间 1px 分隔线。
///
/// 上行 BPM、下行 PPQ 都可点击编辑（同一时间只有一个字段处于编辑态）；
/// 输入变化与失焦分别回传 `*EditChanged` / [`DashboardBlur`]。
pub(super) fn dashboard(ui: &mut egui::Ui, model: &ToolbarModel) {
    let rect = ui.max_rect();
    let split = rect.left() + layout::DASHBOARD_CELL_W;
    ui.painter().rect_filled(
        Rect::from_min_size(
            egui::pos2(split, rect.top() + 8.0),
            Vec2::new(1.0, rect.height() - 16.0),
        ),
        0,
        theme::border(),
    );

    // 非编辑字段清理会话标记：含分组被折叠、输入框未绘制的帧。
    for field in [DashboardField::Bpm, DashboardField::Ppq] {
        if !field.editing(model) {
            reset_metric_edit_session(ui.ctx(), field);
        }
    }

    let cell = Rect::from_min_max(rect.min, egui::pos2(split, rect.bottom()));
    if model.bpm_editing {
        bpm_edit(ui, cell, model);
    } else if model.ppq_editing {
        ppq_edit(ui, cell, model);
    } else {
        metric(ui, cell, &model.bpm, &model.ppq);
        // 上下两行各占半格：BPM（上行）与 PPQ（下行）分别进入自己的编辑态。
        let split_y = cell.center().y;
        let bpm_row = Rect::from_min_max(cell.min, egui::pos2(cell.right(), split_y));
        let ppq_row = Rect::from_min_max(egui::pos2(cell.left(), split_y), cell.max);
        if metric_row(ui, DashboardField::Bpm, bpm_row, model.tips.bpm_cell) {
            toolbar_popup::push_action(ui.ctx(), ToolbarAction::BpmEditToggled);
        }
        if metric_row(ui, DashboardField::Ppq, ppq_row, model.tips.ppq_cell) {
            toolbar_popup::push_action(ui.ctx(), ToolbarAction::PpqEditToggled);
        }
    }
    metric(
        ui,
        Rect::from_min_max(egui::pos2(split, rect.top()), rect.max),
        &model.position,
        &model.time,
    );
}

/// 单行点击区：点击返回 `true`。
fn metric_row(ui: &mut egui::Ui, field: DashboardField, rect: Rect, tip: HoverTip) -> bool {
    let response = ui.interact(
        rect,
        egui::Id::unique(("lumino.toolbar.metric-cell", field.key())),
        Sense::click(),
    );
    widgets::hover_tip(response, tip).clicked()
}

/// BPM 编辑态：上行输入框，下行仍显示 PPQ。
fn bpm_edit(ui: &mut egui::Ui, cell: Rect, model: &ToolbarModel) {
    // 下行保持 PPQ 文本可见（与 PPQ 编辑时上行保留 BPM 对称）。
    metric_line(
        ui,
        cell,
        cell.top() + 31.0,
        &model.ppq,
        FontId::proportional(theme::font::SMALL),
        theme::text_muted(),
    );
    let field = metric_field_rect(cell, cell.top() + 15.0);
    metric_edit(ui, DashboardField::Bpm, cell, field, model);
}

/// PPQ 编辑态：上行仍显示 BPM，下行是输入框。
fn ppq_edit(ui: &mut egui::Ui, cell: Rect, model: &ToolbarModel) {
    metric_line(
        ui,
        cell,
        cell.top() + 15.0,
        &model.bpm,
        FontId::proportional(theme::font::BODY),
        theme::accent(),
    );
    let field = metric_field_rect(cell, cell.top() + 31.0);
    metric_edit(ui, DashboardField::Ppq, cell, field, model);
}

/// 输入框矩形（单元格宽 − 16，高 16，纵向中心在 `center_y`）。
fn metric_field_rect(cell: Rect, center_y: f32) -> Rect {
    Rect::from_center_size(
        egui::pos2(cell.center().x, center_y),
        Vec2::new((layout::DASHBOARD_CELL_W - 16.0).max(20.0), 16.0),
    )
}

/// 字段编辑框：抢焦点 / 文本变化 / 失焦上报 / 错误文本（BPM 与 PPQ 共用）。
fn metric_edit(
    ui: &mut egui::Ui,
    field: DashboardField,
    cell: Rect,
    rect: Rect,
    model: &ToolbarModel,
) {
    let id = egui::Id::unique(("lumino.toolbar.metric-input", field.key()));
    // 每次进入编辑只抢一次焦点（否则用户点到别处会被立刻抢回来）。
    // 标记在字段退出编辑时清除（`dashboard`），保证每个编辑会话都能重新聚焦。
    let focused_once = metric_focus_once_id(field);
    if !ui
        .ctx()
        .data(|d| d.get_temp::<bool>(focused_once).unwrap_or(false))
    {
        ui.ctx().data_mut(|d| d.insert_temp(focused_once, true));
        ui.memory_mut(|m| m.request_focus(id));
    }

    let mut buffer = field.buffer(model).to_owned();
    let response = ui.put(
        rect,
        egui::TextEdit::singleline(&mut buffer)
            .id(id)
            .font(FontId::proportional(theme::font::SMALL))
            .margin(Vec2::new(4.0, 0.0))
            .align(egui::Align2::CENTER_CENTER),
    );
    if response.changed() {
        let action = match field {
            DashboardField::Bpm => ToolbarAction::BpmEditChanged(buffer),
            DashboardField::Ppq => ToolbarAction::PpqEditChanged(buffer),
        };
        toolbar_popup::push_action(ui.ctx(), action);
    }
    // 重新获得焦点（用户点回输入框）→ 允许下一次失焦再上报。
    if response.gained_focus() {
        ui.ctx()
            .data_mut(|d| d.remove_temp::<bool>(metric_blur_reported_id(field)));
    }
    // 失焦即收口：回车 / Tab / 点击任意其它位置（含工具栏空白处）都会让
    // TextEdit 交出焦点。只认「回车」会静默丢弃点击别处前输入的内容。
    // Esc 是唯一例外：放弃本次编辑，不写入。
    //
    // `lost_focus` 会连续两帧为真（egui 为同帧焦点转移留的补偿窗口），
    // 用会话标记去重，保证一次失焦只提交一次。
    let blur_reported = ui.ctx().data(|d| {
        d.get_temp::<bool>(metric_blur_reported_id(field))
            .unwrap_or(false)
    });
    if response.lost_focus() && !blur_reported {
        ui.ctx()
            .data_mut(|d| d.insert_temp(metric_blur_reported_id(field), true));
        let outcome = if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            MetricBlur::Cancel
        } else {
            MetricBlur::Commit
        };
        push_dashboard_blur(ui.ctx(), DashboardBlur { field, outcome });
    }

    if let Some(error) = field.error(model) {
        // 错误文本可能比单元格宽（如「数值 1920960 超出允许范围 24..32767」）：
        // 必须裁剪在本列内，否则会压到右侧「位置 / 时间」列上。
        ui.painter().with_clip_rect(cell).text(
            egui::pos2(cell.center().x, cell.bottom() - 6.0),
            Align2::CENTER_CENTER,
            error,
            FontId::proportional(9.0),
            theme::danger(),
        );
    }
}

/// 单列：上行强调色数值 + 下行弱色说明。
fn metric(ui: &egui::Ui, rect: Rect, value: &str, sub: &str) {
    metric_line(
        ui,
        rect,
        rect.top() + 15.0,
        value,
        FontId::proportional(theme::font::BODY),
        theme::accent(),
    );
    metric_line(
        ui,
        rect,
        rect.top() + 31.0,
        sub,
        FontId::proportional(theme::font::SMALL),
        theme::text_muted(),
    );
}

/// 单元格内单行居中文本。
fn metric_line(ui: &egui::Ui, cell: Rect, y: f32, text: &str, font: FontId, color: egui::Color32) {
    ui.painter().text(
        egui::pos2(cell.center().x, y),
        Align2::CENTER_CENTER,
        text,
        font,
        color,
    );
}
