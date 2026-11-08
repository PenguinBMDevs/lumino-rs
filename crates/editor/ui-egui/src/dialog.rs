//! 对话框：未保存更改确认（整窗）+ 动作通道。
//!
//! 几何来自生产 iced `view/save_confirm_dialog.rs`：
//! - 自制标题栏 30px（系统原生标题栏时跳过）；
//! - 提示语 16px、垂直居中于内容区顶 + 90px；
//! - 三个按钮 90×35、间距 10，水平居中于内容区，顶边 = 内容区顶 + 126。
//!
//! 颜色全部来自 [`DialogPalette`]（生产 iced `extended_palette` 映射），
//! 因此深色 / 浅色主题都能整窗接管。
//!
//! 三个按钮与标题栏红绿灯都回传 [`SaveConfirmAction`]，生产侧再翻译成
//! `Message::SaveConfirm(...)` / `window::Event`；标题栏空白区按下即请求拖动
//! 窗口，保持与 iced 自制标题栏一致的行为。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Rect, Stroke, Vec2};

use crate::chrome::{DialogPalette, SaveConfirmModel};
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;
use crate::widgets;

/// 主按钮文字色由 `theme::on_accent()` 按主题亮度决定（深色强调 → 深字，浅色 → 白字）。
const ACTION_ID: &str = "lumino.save-confirm.action";
const RECTS_ID: &str = "lumino.save-confirm.rects";

/// 保存确认对话框产生的动作（不依赖 iced）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveConfirmAction {
    /// 保存后继续。
    Save,
    /// 放弃更改直接关闭。
    Discard,
    /// 取消关闭，继续编辑。
    Cancel,
    /// 标题栏空白区按下：拖动窗口。
    DragWindow,
    /// 最小化窗口。
    Minimize,
    /// 最大化 / 还原窗口。
    ToggleMaximize,
    /// 关闭对话框窗口（等价于取消）。
    CloseWindow,
}

/// 记录本帧要回传给生产的动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: SaveConfirmAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<SaveConfirmAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<SaveConfirmAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<SaveConfirmAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 便于测试：本帧记录的控件矩形（`dialog:save` / `dialog:discard` /
/// `dialog:cancel` / `dialog:drag` / `dialog:close` / `dialog:max` / `dialog:min`）。
pub fn rect(ctx: &egui::Context, key: &str) -> Option<Rect> {
    let rects: Vec<(String, Rect)> = ctx
        .data(|d| d.get_temp(Id::unique(RECTS_ID)))
        .unwrap_or_default();
    rects
        .into_iter()
        .find(|(name, _)| name == key)
        .map(|(_, rect)| rect)
}

fn record_rect(ctx: &egui::Context, key: &str, rect: Rect) {
    let mut rects: Vec<(String, Rect)> = ctx
        .data(|d| d.get_temp(Id::unique(RECTS_ID)))
        .unwrap_or_default();
    if !rects.iter().any(|(name, _)| name == key) {
        rects.push((key.to_owned(), rect));
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(RECTS_ID), rects));
}

/// 对话框草稿状态（场景入口用）。
#[derive(Default)]
pub struct DialogState {
    /// 图标缓存（必须跨帧存活，否则纹理会在渲染前被释放）。
    pub icons: IconCache,
}

/// 场景入口：与生产同文案、同深色调色板的保存确认对话框。
pub fn show(ui: &mut egui::Ui, state: &mut DialogState) {
    let model = SaveConfirmModel {
        titlebar: true,
        palette: DialogPalette::DARK,
        message: "是否保留未保存的更改?".to_owned(),
        save: "保存".to_owned(),
        close: "关闭".to_owned(),
        cancel: "取消".to_owned(),
    };
    show_model(ui, &model, &mut state.icons);
}

/// 按只读模型渲染整窗（真实窗口叠加层与场景共用）。
pub fn show_model(ui: &mut egui::Ui, model: &SaveConfirmModel, icons: &mut IconCache) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    ui.ctx()
        .data_mut(|d| d.remove_temp::<Vec<(String, Rect)>>(Id::unique(RECTS_ID)));

    // `egui_kittest`（以及部分宿主）给到的 Ui 带 8px 外缩进；Panel 需要铺满视口，
    // 否则 band 会整体内缩、与 iced 原位置错开。
    let full = ui.ctx().viewport_rect();
    let mut root = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(full)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    root.set_clip_rect(full);
    root.spacing_mut().item_spacing = egui::Vec2::ZERO;

    if model.titlebar {
        egui::Panel::top("save-confirm-titlebar")
            .exact_size(layout::TITLEBAR_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(crate::chrome::panel_frame(model.palette.titlebar))
            .show(&mut root, |ui| titlebar(ui, model.palette, icons));
    }

    egui::CentralPanel::default()
        .frame(crate::chrome::panel_frame(model.palette.background))
        .show(&mut root, |ui| content(ui, model));
}

fn titlebar(ui: &mut egui::Ui, palette: DialogPalette, cache: &mut IconCache) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, palette.titlebar);
    ui.spacing_mut().item_spacing.x = 0.0;

    // 标题栏空白区：按下即请求拖动窗口（红绿灯区域除外）。
    let drag = Rect::from_min_max(
        rect.min,
        egui::pos2(
            (rect.right() - layout::TRAFFIC_BTN_W * 3.0).max(rect.left()),
            rect.bottom(),
        ),
    );
    record_rect(ui.ctx(), "dialog:drag", drag);
    let pointer_in_drag = ui
        .input(|i| i.pointer.interact_pos())
        .is_some_and(|pos| drag.contains(pos));
    let pressed = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
    if pointer_in_drag && pressed {
        push_action(ui.ctx(), SaveConfirmAction::DragWindow);
    }

    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let close = widgets::traffic_button(ui, cache, Icon::WindowClose, true);
        record_rect(ui.ctx(), "dialog:close", close.rect);
        if close.clicked() {
            push_action(ui.ctx(), SaveConfirmAction::CloseWindow);
        }
        let max = widgets::traffic_button(ui, cache, Icon::WindowMax, false);
        record_rect(ui.ctx(), "dialog:max", max.rect);
        if max.clicked() {
            push_action(ui.ctx(), SaveConfirmAction::ToggleMaximize);
        }
        let min = widgets::traffic_button(ui, cache, Icon::WindowMin, false);
        record_rect(ui.ctx(), "dialog:min", min.rect);
        if min.clicked() {
            push_action(ui.ctx(), SaveConfirmAction::Minimize);
        }
    });
}

fn content(ui: &mut egui::Ui, model: &SaveConfirmModel) {
    let palette = model.palette;
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, palette.background);

    // 生产 iced：提示 + 按钮的主列在内容区垂直居中（`center_y(Fill)`）。
    // 场景视口足够高时保持既有基线偏移（90 / 126）；对话框窗口按内容测量高度
    // （169 逻辑高），固定偏移会把按钮推出窗口底边——仅在会越界时整体上移，
    // 保证底部仍留 24px 内边距。
    let button_w = 90.0;
    let button_h = 35.0;
    let gap = 10.0;
    let prompt_center_y = rect.top() + 90.0;
    let ideal_buttons_top = rect.top() + 126.0;
    let max_buttons_top = rect.bottom() - 24.0 - button_h;
    let shift = (ideal_buttons_top - max_buttons_top).max(0.0);
    let prompt_center_y = prompt_center_y - shift;
    ui.painter().text(
        egui::pos2(rect.center().x, prompt_center_y),
        Align2::CENTER_CENTER,
        &model.message,
        // 生产 iced：提示语 16px（`text(...).size(16)`）。
        FontId::proportional(theme::font::SECTION),
        palette.text,
    );

    let total = button_w * 3.0 + gap * 2.0;
    let start_x = rect.center().x - total / 2.0;
    let y = ideal_buttons_top - shift;

    let save = button(
        ui,
        Rect::from_min_size(egui::pos2(start_x, y), Vec2::new(button_w, button_h)),
        &model.save,
        palette.primary,
        palette.primary_hover,
        theme::on_accent(),
        palette.primary_border,
        "dialog:save",
    );
    if save.clicked() {
        push_action(ui.ctx(), SaveConfirmAction::Save);
    }
    let discard = button(
        ui,
        Rect::from_min_size(
            egui::pos2(start_x + button_w + gap, y),
            Vec2::new(button_w, button_h),
        ),
        &model.close,
        palette.secondary,
        palette.secondary_hover,
        palette.text,
        palette.secondary_border,
        "dialog:discard",
    );
    if discard.clicked() {
        push_action(ui.ctx(), SaveConfirmAction::Discard);
    }
    let cancel = button(
        ui,
        Rect::from_min_size(
            egui::pos2(start_x + (button_w + gap) * 2.0, y),
            Vec2::new(button_w, button_h),
        ),
        &model.cancel,
        palette.secondary,
        palette.secondary_hover,
        palette.text,
        palette.secondary_border,
        "dialog:cancel",
    );
    if cancel.clicked() {
        push_action(ui.ctx(), SaveConfirmAction::Cancel);
    }
}

/// 对话框按钮：`90×35`、圆角 4、1px 边框、悬停换底色。
#[allow(clippy::too_many_arguments)]
fn button(
    ui: &mut egui::Ui,
    rect: Rect,
    text: &str,
    fill: Color32,
    hover: Color32,
    text_color: Color32,
    border: Color32,
    key: &str,
) -> egui::Response {
    record_rect(ui.ctx(), key, rect);
    let response = ui.interact(rect, Id::unique(key), egui::Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        if response.hovered() { hover } else { fill },
        Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(theme::font::LABEL),
        text_color,
    );
    response
}
