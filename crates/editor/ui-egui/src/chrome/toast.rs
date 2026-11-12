//! 右下角浮层：Toast 通知 + 首次使用帮助提示。
//!
//! 几何取自 iced `toast.rs` / `view/help_tip.rs`：
//! - Toast：右下角，避开状态栏 36、右边距 16；卡片内边距 10、圆角 6、间距 6、
//!   最大宽 420；图标 16 + 文案 14，白字；
//! - 帮助提示：右下角内边距 16，卡片宽 360、内边距 14、圆角 8、1px 边框；
//!   标题 15、正文 12、两个按钮。

use egui::{Align2, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Vec2};

use super::{HelpTipModel, ToastModel};

/// Toast 卡片内边距。
const TOAST_PAD: f32 = 10.0;
/// Toast 间距。
const TOAST_GAP: f32 = 6.0;
/// Toast 最大宽度。
const TOAST_MAX_W: f32 = 420.0;
/// Toast 与状态栏的距离。
const TOAST_BOTTOM: f32 = 36.0;
/// Toast 右边距。
const TOAST_RIGHT: f32 = 16.0;

/// 帮助提示宽度。
const TIP_W: f32 = 360.0;
/// 帮助提示内边距。
const TIP_PAD: f32 = 14.0;
/// 帮助提示外边距。
const TIP_MARGIN: f32 = 16.0;

/// 帮助提示上的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTipAction {
    /// 打开帮助对话框。
    OpenHelp,
    /// 关闭提示（标记已读）。
    Dismiss,
}

/// 绘制 Toast 叠加层（右下角，自下而上堆叠）。
pub fn show_toasts(ui: &egui::Ui, full: Rect, model: &ToastModel) {
    if model.cards.is_empty() {
        return;
    }
    let mut bottom = full.bottom() - TOAST_BOTTOM;
    for card in model.cards.iter().rev() {
        let text_w = measure(ui, &card.message, 14.0);
        let width = (text_w + 16.0 + 8.0 + TOAST_PAD * 2.0).min(TOAST_MAX_W);
        let height = 14.0 + TOAST_PAD * 2.0;
        let rect = Rect::from_min_size(
            egui::pos2(full.right() - TOAST_RIGHT - width, bottom - height),
            Vec2::new(width, height),
        );
        ui.painter().rect(
            rect,
            CornerRadius::same(6),
            card.background,
            Stroke::NONE,
            StrokeKind::Inside,
        );
        ui.painter().text(
            egui::pos2(rect.left() + TOAST_PAD, rect.center().y),
            Align2::LEFT_CENTER,
            &card.icon,
            FontId::proportional(16.0),
            card.text,
        );
        ui.painter().text(
            egui::pos2(rect.left() + TOAST_PAD + 16.0 + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            &card.message,
            FontId::proportional(14.0),
            card.text,
        );
        bottom = rect.top() - TOAST_GAP;
    }
}

/// 绘制首次使用帮助提示；返回本帧动作。
pub fn show_help_tip(ui: &egui::Ui, full: Rect, model: &HelpTipModel) -> Option<HelpTipAction> {
    let body_w = TIP_W - TIP_PAD * 2.0;
    let body_h = wrapped_height(ui, &model.body, 12.0, body_w);
    let height = TIP_PAD * 2.0 + 15.0 + 6.0 + body_h + 10.0 + 26.0;
    let rect = Rect::from_min_size(
        egui::pos2(
            full.right() - TIP_MARGIN - TIP_W,
            full.bottom() - TIP_MARGIN - height,
        ),
        Vec2::new(TIP_W, height),
    );
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        crate::theme::toolbar_bg(),
        Stroke::new(1.0, crate::theme::border()),
        StrokeKind::Inside,
    );
    let painter = ui.painter().with_clip_rect(rect);
    painter.text(
        egui::pos2(rect.left() + TIP_PAD, rect.top() + TIP_PAD),
        Align2::LEFT_TOP,
        &model.title,
        FontId::proportional(15.0),
        crate::theme::text(),
    );
    let body_rect = Rect::from_min_size(
        egui::pos2(rect.left() + TIP_PAD, rect.top() + TIP_PAD + 15.0 + 6.0),
        Vec2::new(body_w, body_h),
    );
    painter.galley(
        body_rect.min,
        ui.painter().layout(
            model.body.clone(),
            FontId::proportional(12.0),
            crate::theme::text_muted(),
            body_w,
        ),
        crate::theme::text_muted(),
    );

    // 两个按钮（左「查看帮助」/ 右「知道了」）。
    let button_y = rect.bottom() - TIP_PAD - 13.0;
    let help_w = measure(ui, &model.help_label, 12.0) + 24.0;
    let dismiss_w = measure(ui, &model.dismiss_label, 12.0) + 24.0;
    let help = Rect::from_min_size(
        egui::pos2(rect.left() + TIP_PAD, button_y - 13.0),
        Vec2::new(help_w, 26.0),
    );
    let dismiss = Rect::from_min_size(
        egui::pos2(help.right() + 8.0, button_y - 13.0),
        Vec2::new(dismiss_w, 26.0),
    );
    let mut action = None;
    for (button, label, act, primary) in [
        (help, &model.help_label, HelpTipAction::OpenHelp, false),
        (dismiss, &model.dismiss_label, HelpTipAction::Dismiss, true),
    ] {
        let response = ui.interact(
            button,
            egui::Id::unique(("lumino.help-tip", matches!(act, HelpTipAction::Dismiss))),
            Sense::click(),
        );
        let fill = if primary {
            crate::theme::accent()
        } else {
            crate::theme::toolbar_bg()
        };
        ui.painter()
            .rect_filled(button, CornerRadius::same(4), fill);
        ui.painter().text(
            button.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(12.0),
            if primary {
                crate::theme::on_accent()
            } else {
                crate::theme::text()
            },
        );
        if response.clicked() {
            action = Some(act);
        }
    }
    // 提示只占它自己的卡片，不吃别处的点击。
    if action.is_some() {
        crate::chrome::menu::extend_blocked(ui.ctx(), [rect]);
    }
    action
}

/// 单行文本宽度。
fn measure(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(
            text.to_owned(),
            FontId::proportional(size),
            crate::theme::text(),
        )
        .size()
        .x
}

/// 折行后的高度。
fn wrapped_height(ui: &egui::Ui, text: &str, size: f32, width: f32) -> f32 {
    ui.painter()
        .layout(
            text.to_owned(),
            FontId::proportional(size),
            crate::theme::text(),
            width,
        )
        .size()
        .y
}
