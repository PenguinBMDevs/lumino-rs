//! 工具栏左侧分组：录制 / 走带 / 循环 / 撤销重做 / 检测仪表盘。
//!
//! 这里只负责「画 + 命中」，点击一律经 `toolbar_popup::push_action` 回传；
//! 文案与状态由 [`ToolbarModel`] 提供，本 crate 不引 i18n。

use egui::{Align2, FontId, Rect, Sense, Vec2};

use super::super::ToolbarModel;
use super::super::toolbar_popup::{self, ToolbarAction};
use super::{
    PpqBlur, ppq_blur_reported_id, ppq_focus_once_id, push_ppq_blur, reset_ppq_edit_session,
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
/// PPQ 处于编辑态时，第一列下半行换成输入框（与 上一代 `ppq_edit_column` 同义）；
/// 输入变化与失焦分别回传 `PpqEditChanged` / [`PpqBlur`]。
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

    let ppq_cell = Rect::from_min_max(rect.min, egui::pos2(split, rect.bottom()));
    if model.ppq_editing {
        ppq_edit(ui, ppq_cell, model);
    } else {
        // 编辑会话已结束：清掉会话标记（抢焦点 / 失焦已上报），下一次进入
        // 编辑重新抢焦点。egui 临时数据跨帧留存，不清理会让第二个编辑会话的
        // 输入框拿不到键盘。
        reset_ppq_edit_session(ui.ctx());
        metric(ui, ppq_cell, &model.bpm, &model.ppq);
        // 只有 PPQ 那一行可点：BPM 是只读显示，命中它不得进入 PPQ 编辑
        // （上一代 也只让 `PPQ 1920` 行可点）。
        let ppq_row = Rect::from_min_max(
            egui::pos2(ppq_cell.left(), ppq_cell.center().y),
            ppq_cell.max,
        );
        let response = ui.interact(
            ppq_row,
            egui::Id::unique("lumino.toolbar.ppq-cell"),
            Sense::click(),
        );
        let response = widgets::hover_tip(response, model.tips.ppq_cell);
        if response.clicked() {
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

/// PPQ 编辑态：上行仍显示 BPM，下行是输入框（宽 80−8），错误文本在最底部。
fn ppq_edit(ui: &mut egui::Ui, rect: Rect, model: &ToolbarModel) {
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 15.0),
        Align2::CENTER_CENTER,
        &model.bpm,
        FontId::proportional(theme::font::BODY),
        theme::accent(),
    );

    let field = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 31.0),
        Vec2::new((layout::DASHBOARD_CELL_W - 16.0).max(20.0), 16.0),
    );
    let id = egui::Id::unique("lumino.toolbar.ppq-input");
    // 每次进入编辑只抢一次焦点（否则用户点到别处会被立刻抢回来）。
    // 标记在非编辑态清除（`dashboard`），保证每个编辑会话都能重新聚焦。
    let focused_once = ppq_focus_once_id();
    if !ui
        .ctx()
        .data(|d| d.get_temp::<bool>(focused_once).unwrap_or(false))
    {
        ui.ctx().data_mut(|d| d.insert_temp(focused_once, true));
        ui.memory_mut(|m| m.request_focus(id));
    }

    let mut buffer = model.ppq_buffer.clone();
    let response = ui.put(
        field,
        egui::TextEdit::singleline(&mut buffer)
            .id(id)
            .font(FontId::proportional(theme::font::SMALL))
            .margin(Vec2::new(4.0, 0.0))
            .align(egui::Align2::CENTER_CENTER),
    );
    if response.changed() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::PpqEditChanged(buffer));
    }
    // 重新获得焦点（用户点回输入框）→ 允许下一次失焦再上报。
    if response.gained_focus() {
        ui.ctx()
            .data_mut(|d| d.remove_temp::<bool>(ppq_blur_reported_id()));
    }
    // 失焦即收口：回车 / Tab / 点击任意其它位置（含工具栏空白处）都会让
    // TextEdit 交出焦点。只认「回车」会静默丢弃点击别处前输入的内容。
    // Esc 是唯一例外：放弃本次编辑，不写入。
    //
    // `lost_focus` 会连续两帧为真（egui 为同帧焦点转移留的补偿窗口），
    // 用会话标记去重，保证一次失焦只提交一次。
    let blur_reported = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(ppq_blur_reported_id()).unwrap_or(false));
    if response.lost_focus() && !blur_reported {
        ui.ctx()
            .data_mut(|d| d.insert_temp(ppq_blur_reported_id(), true));
        let blur = if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            PpqBlur::Cancel
        } else {
            PpqBlur::Commit
        };
        push_ppq_blur(ui.ctx(), blur);
    }

    if let Some(error) = &model.ppq_error {
        // 错误文本可能比单元格宽（如「数值 1920960 超出允许范围 24..32767」）：
        // 必须裁剪在本列内，否则会压到右侧「位置 / 时间」列上。
        ui.painter().with_clip_rect(rect).text(
            egui::pos2(rect.center().x, rect.bottom() - 6.0),
            Align2::CENTER_CENTER,
            error,
            FontId::proportional(9.0),
            theme::danger(),
        );
    }
}

/// 单列：上行强调色数值 + 下行弱色说明。
fn metric(ui: &egui::Ui, rect: Rect, value: &str, sub: &str) {
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 15.0),
        Align2::CENTER_CENTER,
        value,
        FontId::proportional(theme::font::BODY),
        theme::accent(),
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 31.0),
        Align2::CENTER_CENTER,
        sub,
        FontId::proportional(theme::font::SMALL),
        theme::text_muted(),
    );
}
