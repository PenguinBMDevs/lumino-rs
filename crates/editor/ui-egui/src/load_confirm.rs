//! 加载确认对话框（`DialogType::LoadConfirm`）。
//!
//! 内边距 24：三行文件信息（14px 文件名 / 14px 大小 / 12px 弱色路径，行间 6）、
//! 13px 弱色提示（上方 16）、按钮行（上方 24）：100×34「取消」（`background.weak`
//! 底）+ 12 + 100×34 主色「加载」。
//!
//! 只回传确认 / 取消，实际加载由生产侧完成。

use egui::{Align, Color32, CornerRadius, RichText, Vec2};

use crate::chrome::LoadConfirmModel;
use crate::theme;

const ACTION_ID: &str = "lumino.load-confirm.action";
/// 内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 14px 文本行高（含 旧版 1.5 倍行高）。
const LINE_H: f32 = 21.0;
/// 12 / 13px 文本行高。
const SMALL_H: f32 = 18.0;
/// 行间距（生产 `space().height(6)`）。
const LINE_GAP: f32 = 6.0;
/// 按钮（生产 100 宽、`padding([8,32])`）。
const BUTTON_W: f32 = 100.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;

/// 加载确认对话框产生的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadConfirmAction {
    /// 确认加载。
    Confirm,
    /// 取消 / 关闭。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: LoadConfirmAction) {
    if ctx
        .data(|d| d.get_temp::<Option<LoadConfirmAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<LoadConfirmAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<LoadConfirmAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &LoadConfirmModel) {
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

    let mut y = inner.top();
    for (text, size, color) in [
        (&model.file_line, 14.0, theme::text()),
        (&model.size_line, 14.0, theme::text()),
        (&model.path_line, 12.0, theme::text_muted()),
    ] {
        let line_h = if size >= 14.0 { LINE_H } else { SMALL_H };
        label_block(
            ui,
            egui::Rect::from_min_size(
                egui::pos2(inner.left(), y),
                Vec2::new(inner.width(), line_h),
            ),
            text,
            size,
            color,
        );
        y += line_h + LINE_GAP;
    }

    y += 16.0 - LINE_GAP;
    label_block(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.left(), y),
            Vec2::new(inner.width(), SMALL_H),
        ),
        &model.hint,
        13.0,
        theme::text_muted(),
    );
    y += SMALL_H + 24.0;

    // 按钮行：左对齐。
    let cancel = egui::Button::new(
        RichText::new(&model.cancel)
            .size(theme::font::LABEL)
            .color(theme::text()),
    )
    .fill(theme::toolbar_bg())
    .corner_radius(CornerRadius::same(4));
    if ui
        .put(
            egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(BUTTON_W, BUTTON_H)),
            cancel,
        )
        .clicked()
    {
        push_action(ui.ctx(), LoadConfirmAction::Cancel);
    }

    let confirm = egui::Button::new(
        RichText::new(&model.confirm)
            .size(theme::font::LABEL)
            .color(theme::on_accent()),
    )
    .fill(theme::accent())
    .corner_radius(CornerRadius::same(4));
    if ui
        .put(
            egui::Rect::from_min_size(
                egui::pos2(inner.left() + BUTTON_W + BUTTON_GAP, y),
                Vec2::new(BUTTON_W, BUTTON_H),
            ),
            confirm,
        )
        .clicked()
    {
        push_action(ui.ctx(), LoadConfirmAction::Confirm);
    }
}

/// 固定矩形内的左对齐文本标签（保留可访问树节点）。
fn label_block(ui: &mut egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
