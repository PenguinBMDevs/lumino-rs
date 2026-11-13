//! 自动保存恢复对话框（`DialogType::AutosaveRecover`，DEBT-12）。
//!
//! 对齐生产旧视图：整窗 `base` 底、内边距 24；内容整体居中——
//! 标题 17px、路径 / 修改时间 / 说明 13px（次要色）、间距 10/4/8/24；
//! 按钮「恢复自动保存」130×34（主色）、10px、「不恢复」100×34（弱底）。
//!
//! 文件系统操作全部由 Runner 完成，本层只回传 `Restore` / `Discard`。

use egui::{Color32, CornerRadius, RichText, Vec2};

use crate::chrome::AutosaveRecoverModel;
use crate::theme;

const ACTION_ID: &str = "lumino.autosave-recover.action";
/// 整窗内边距（生产 `padding(24)`）。
const PAD: f32 = 24.0;
/// 按钮尺寸（生产 130×34 / 100×34，间距 10）。
const RESTORE_W: f32 = 130.0;
const DISCARD_W: f32 = 100.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 10.0;

/// 自动保存恢复对话框产生的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosaveRecoverAction {
    /// 按「未保存工程」载入自动保存。
    Restore,
    /// 不恢复：文件改名保留，下次启动不再询问。
    Discard,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: AutosaveRecoverAction) {
    if ctx
        .data(|d| d.get_temp::<Option<AutosaveRecoverAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<AutosaveRecoverAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<AutosaveRecoverAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &AutosaveRecoverModel) {
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

    // 生产：`column![title, 10, path, 4, time, 8, hint, 24, buttons]`
    // 在 `container(..).center_x(Fill).center_y(Fill)` 内整体居中。
    let line_h = 18.0;
    let title_h = 24.0;
    let content_h = title_h + 10.0 + line_h + 4.0 + line_h + 8.0 + line_h + 24.0 + BUTTON_H;
    let mut y = inner.top() + ((inner.height() - content_h) * 0.5).max(0.0);
    let left = inner.left();
    let width = inner.width();

    centered_text(
        ui,
        egui::Rect::from_min_size(egui::pos2(left, y), Vec2::new(width, title_h)),
        &model.title,
        17.0,
        theme::text(),
    );
    y += title_h + 10.0;
    centered_text(
        ui,
        egui::Rect::from_min_size(egui::pos2(left, y), Vec2::new(width, line_h)),
        &model.path,
        13.0,
        theme::text_muted(),
    );
    y += line_h + 4.0;
    centered_text(
        ui,
        egui::Rect::from_min_size(egui::pos2(left, y), Vec2::new(width, line_h)),
        &model.time,
        13.0,
        theme::text_muted(),
    );
    y += line_h + 8.0;
    centered_text(
        ui,
        egui::Rect::from_min_size(egui::pos2(left, y), Vec2::new(width, line_h)),
        &model.hint,
        13.0,
        theme::text_muted(),
    );
    y += line_h + 24.0;

    let total = RESTORE_W + BUTTON_GAP + DISCARD_W;
    let mut x = inner.center().x - total * 0.5;
    let restore = egui::Button::new(
        RichText::new(&model.restore)
            .size(theme::font::LABEL)
            .color(Color32::WHITE),
    )
    .fill(theme::accent())
    .corner_radius(CornerRadius::same(4));
    if ui
        .put(
            egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(RESTORE_W, BUTTON_H)),
            restore,
        )
        .clicked()
    {
        push_action(ui.ctx(), AutosaveRecoverAction::Restore);
    }
    x += RESTORE_W + BUTTON_GAP;

    let discard = egui::Button::new(
        RichText::new(&model.discard)
            .size(theme::font::LABEL)
            .color(theme::text_muted()),
    )
    .fill(theme::toolbar_bg())
    .corner_radius(CornerRadius::same(4));
    if ui
        .put(
            egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(DISCARD_W, BUTTON_H)),
            discard,
        )
        .clicked()
    {
        push_action(ui.ctx(), AutosaveRecoverAction::Discard);
    }
}

/// 水平居中的单行文本（进入可访问树）。
fn centered_text(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    text: &str,
    size: f32,
    color: Color32,
) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    ));
    child.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .selectable(false)
            .truncate(),
    );
}
