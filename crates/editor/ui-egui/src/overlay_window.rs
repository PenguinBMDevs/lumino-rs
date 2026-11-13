//! 独立小窗口（启动/加载进度、设备检查警告）的 egui 整窗绘制。
//!
//! 这两类窗口不是 `DialogType` 对话框，但同样只由 egui 绘制（旧版 不再 present），
//! 因此内容必须在这里补齐，否则窗口只剩标题栏。
//!
//! 几何取自 旧版 生产视图（`root/view/overlays.rs`）：进度窗 30px 内边距、
//! 三行 24/16/进度条、居中；设备警告窗 24px 内边距、左对齐三段。

use egui::{Align, Layout, Rect, RichText, Vec2};

use crate::chrome::{DeviceWarningModel, ProgressModel};
use crate::layout;
use crate::theme;

/// 警告窗动作（生产侧再翻译成 `window::Event`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayWindowAction {
    /// 切换「不要再提示我」。
    SuppressToggled,
    /// 确认并关闭（退出）。
    Quit,
    /// 放我进去（`suppress` 为当前勾选状态）。
    Continue {
        /// 是否同时写入「不再提示」。
        suppress: bool,
    },
}

const ACTION_ID: &str = "lumino.overlay-window.action";

/// 记录本帧动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, action: OverlayWindowAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), action));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<OverlayWindowAction> {
    let action = ctx.data(|d| d.get_temp(egui::Id::unique(ACTION_ID)));
    if action.is_some() {
        ctx.data_mut(|d| d.remove::<OverlayWindowAction>(egui::Id::unique(ACTION_ID)));
    }
    action
}

/// 内容区（扣掉自制标题栏 30px）。
fn content_rect(ctx: &egui::Context, titlebar: bool) -> Rect {
    let full = ctx.viewport_rect();
    let top = if titlebar { layout::TITLEBAR_H } else { 0.0 };
    Rect::from_min_max(egui::pos2(full.left(), full.top() + top), full.max)
}

/// 进度窗：标题 / 状态行 / 进度条，全部居中。
pub fn show_progress(ui: &mut egui::Ui, model: &ProgressModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    let area = content_rect(ui.ctx(), model.titlebar);
    let mut root = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(area)
            .layout(Layout::top_down(Align::Center)),
    );
    root.painter().rect_filled(area, 0, theme::base_bg());
    root.add_space(60.0);
    root.label(RichText::new(&model.title).size(24.0).color(theme::text()));
    root.add_space(12.0);
    root.label(
        RichText::new(&model.message)
            .size(16.0)
            .color(theme::text_muted()),
    );
    root.add_space(20.0);
    let width = (area.width() * 0.6).clamp(200.0, 640.0);
    root.add(
        egui::ProgressBar::new(model.progress.clamp(0.0, 1.0))
            .desired_width(width)
            .animate(false),
    );
}

/// 设备检查警告窗：图标 + 标题、详情、勾选框与两个按钮。
pub fn show_device_warning(ui: &mut egui::Ui, model: &DeviceWarningModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    let area = content_rect(ui.ctx(), model.titlebar);
    let mut root = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(area)
            .layout(Layout::top_down(Align::Min)),
    );
    root.painter().rect_filled(area, 0, theme::base_bg());
    let pad = 24.0;
    let mut inner = root.new_child(
        egui::UiBuilder::new()
            .max_rect(area.shrink(pad))
            .layout(Layout::top_down(Align::Min)),
    );
    inner.spacing_mut().item_spacing = Vec2::new(12.0, 20.0);

    inner.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.label(RichText::new("⚠").size(34.0));
        ui.label(
            RichText::new(&model.heading)
                .size(18.0)
                .color(theme::text()),
        );
    });
    inner.label(
        RichText::new(&model.detail)
            .size(14.0)
            .color(theme::text_muted()),
    );

    // 底部动作行：勾选框贴左，两个按钮贴右。
    let row_h = 36.0;
    let row = Rect::from_min_size(
        egui::pos2(inner.max_rect().left(), inner.max_rect().top()),
        Vec2::new(inner.max_rect().width(), row_h),
    );
    let mut actions_ui = inner.new_child(
        egui::UiBuilder::new()
            .max_rect(row)
            .layout(Layout::left_to_right(Align::Center)),
    );
    actions_ui.spacing_mut().item_spacing.x = 12.0;
    let mut suppress = model.suppress;
    if actions_ui
        .checkbox(
            &mut suppress,
            RichText::new(&model.suppress_label).size(14.0),
        )
        .changed()
    {
        push_action(ui.ctx(), OverlayWindowAction::SuppressToggled);
    }
    actions_ui.add_space((row.width() - 320.0).max(0.0));
    if actions_ui
        .button(RichText::new(&model.quit_label).size(14.0))
        .clicked()
    {
        push_action(ui.ctx(), OverlayWindowAction::Quit);
    }
    if actions_ui
        .button(RichText::new(&model.continue_label).size(14.0))
        .clicked()
    {
        push_action(
            ui.ctx(),
            OverlayWindowAction::Continue {
                suppress: model.suppress,
            },
        );
    }
}
