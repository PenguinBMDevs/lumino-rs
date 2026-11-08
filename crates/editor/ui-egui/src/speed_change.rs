//! 变速对话框（`DialogType::SpeedChange`）。
//!
//! 几何对齐生产 `view/speed_change_dialog.rs`：整窗 base 底、内边距 24、
//! 标题 18px；倍率行 = 标签 14px + 12px + 120×33 输入（base 底 + 1px
//! `background.strong` 边框、圆角 4、内边距 [6,10]）；提示 12px
//! （`background.strong.text`）；按钮 34 高（主色「确定」+ 次要「取消」、间距 12）。
//!
//! 各区块按**固定纵向偏移**摆放（标题 0 / 倍率行 50 / 提示 96 / 按钮 145），
//! 因为 iced 的 `column.spacing(4)` 会叠加到每个 `space()` 上，等价于
//! 「title → 24 → row → 12 → hint → 24 → buttons」；egui 的行高不同，用显式
//! 偏移才能得到同一套盒子位置。
//!
//! 只回传原样文本，小数 / 分数解析与范围校验由生产侧负责。

use egui::{Align, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, RichText, Stroke, Vec2};

use crate::chrome::SpeedChangeModel;
use crate::theme;

const ACTION_ID: &str = "lumino.speed-change.action";
/// 整窗内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 标题行高（18px 文本）。
const TITLE_H: f32 = 22.0;
/// 倍率行顶边 / 输入框尺寸（生产 120 宽、`padding([6,10])`）。
const FACTOR_TOP: f32 = 50.0;
const INPUT_W: f32 = 120.0;
const INPUT_H: f32 = 33.0;
/// 提示行顶边 / 行高。
const HINT_TOP: f32 = 101.0;
const HINT_H: f32 = 15.0;
/// 按钮行顶边 / 高（生产 `padding([8,24])` + 14px 文本）。
const BUTTONS_TOP: f32 = 145.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;

/// 变速对话框产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeedChangeAction {
    /// 倍率文本变化（原样回传）。
    FactorChanged(String),
    /// 确定：生产侧解析倍率并应用到选中音符。
    Confirm,
    /// 取消：关闭对话框。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: SpeedChangeAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<SpeedChangeAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<SpeedChangeAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<SpeedChangeAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &SpeedChangeModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    // 自制标题栏（红绿灯 / 拖动）由 iced 绘制：egui 只画标题栏以下内容区。
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    label_block(
        ui,
        Rect::from_min_size(inner.min, Vec2::new(inner.width(), TITLE_H)),
        &model.title,
        18.0,
        theme::text(),
    );
    factor_row(ui, inner, model);
    label_block(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left(), inner.top() + HINT_TOP),
            Vec2::new(inner.width(), HINT_H),
        ),
        &model.hint,
        12.0,
        // 生产提示色是 ackground.strong.text（亮色 = 纯黑），实测最深像素为 0，
        // 用正文色对齐；次级灰在这条 12px 文案上会明显偏淡。
        theme::text(),
    );
    buttons(ui, inner, model);
}

/// 倍率行：标签（14px）+ 12px + 120×33 输入框。
fn factor_row(ui: &mut egui::Ui, inner: Rect, model: &SpeedChangeModel) {
    let mut buffer = model.factor.clone();
    let row_y = inner.top() + FACTOR_TOP;
    // 标签宽度先量出来，输入框才能精确落在「标签 + 12」处（与 iced 同几何）。
    let label_width = ui
        .painter()
        .layout_no_wrap(
            model.factor_label.clone(),
            FontId::proportional(14.0),
            theme::text(),
        )
        .size()
        .x
        .ceil();
    let mut label = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                egui::pos2(inner.left(), row_y),
                Vec2::new(label_width, INPUT_H),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    label.add(egui::Label::new(
        RichText::new(&model.factor_label)
            .size(14.0)
            .color(theme::text()),
    ));

    // 外框手绘：`Frame` 会被 `TextEdit` 的行高撑高（实测比生产高 5px），
    // 这里按生产的 120×33 固定盒子，文本放进内缩后的子 `Ui`。
    let input_rect = Rect::from_min_size(
        egui::pos2(inner.left() + label_width + 12.0, row_y),
        Vec2::new(INPUT_W, INPUT_H),
    );
    ui.painter().rect(
        input_rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut text_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(input_rect.shrink2(Vec2::new(10.0, 6.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = text_ui
        .push_id("speed-change-factor", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .font(FontId::proportional(14.0))
                    .text_color(theme::text())
                    .hint_text(
                        RichText::new(&model.factor_hint)
                            .size(14.0)
                            .color(theme::text_dim()),
                    )
                    .desired_width(INPUT_W - 22.0),
            )
        })
        .inner;
    if response.changed() {
        push_action(ui.ctx(), SpeedChangeAction::FactorChanged(buffer));
    }
}

/// 按钮：确定（主色）+ 12 + 取消（次要）。
fn buttons(ui: &mut egui::Ui, inner: Rect, model: &SpeedChangeModel) {
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                egui::pos2(inner.left(), inner.top() + BUTTONS_TOP),
                Vec2::new(inner.width(), BUTTON_H),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 0.0;
    if button(&mut row, &model.ok, theme::accent(), theme::on_accent()).clicked() {
        push_action(ui.ctx(), SpeedChangeAction::Confirm);
    }
    row.add_space(BUTTON_GAP);
    if button(&mut row, &model.cancel, theme::toolbar_bg(), theme::text()).clicked() {
        push_action(ui.ctx(), SpeedChangeAction::Cancel);
    }
}

/// 固定矩形内的左对齐文本标签（保留可访问树节点）。
fn label_block(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32, color: egui::Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}

fn button(
    ui: &mut egui::Ui,
    label: &str,
    fill: egui::Color32,
    text: egui::Color32,
) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    // 生产用 `padding([8,24])`：宽度随文本，高度固定 34。
    let width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(14.0), text)
        .size()
        .x
        .ceil()
        + 48.0;
    ui.add_sized(Vec2::new(width, BUTTON_H), button)
}
