//! 自定义 Tempo BPM 上限对话框（独立 OS 窗口；DRAW-045）。
//!
//! 版式沿用旧设置页的输入面板：整窗 base 底、内边距 24、标题 18px；
//! 标签 14px + 12px + 120×33 输入框；说明 12px；错误行 12px（危险色）；
//! 按钮 34 高（主色「确定」+ 次要「取消」、间距 12）。
//!
//! 只回传原样文本，解析与校验由生产侧对话框状态负责。

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Stroke, Vec2,
};

use crate::chrome::TempoMaxBpmModel;
use crate::theme;

const ACTION_ID: &str = "lumino.tempo-max-bpm.action";
/// 整窗内边距。
const PAD: f32 = 24.0;
/// 标题行高（18px 文本）。
const TITLE_H: f32 = 22.0;
/// 输入行顶边 / 输入框尺寸。
const INPUT_TOP: f32 = 50.0;
const INPUT_W: f32 = 120.0;
const INPUT_H: f32 = 33.0;
/// 说明行顶边 / 行高。
const HINT_TOP: f32 = 101.0;
const HINT_H: f32 = 15.0;
/// 错误行顶边 / 行高（仅在有错误时绘制）。
const ERROR_TOP: f32 = 122.0;
const ERROR_H: f32 = 15.0;
/// 按钮行顶边 / 高。
const BUTTONS_TOP: f32 = 152.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;

/// 「自定义 BPM 上限」对话框产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempoMaxBpmAction {
    /// 输入框文本变化（原样回传）。
    Input(String),
    /// 确定：生产侧解析并应用。
    Confirm,
    /// 取消：关闭对话框。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: TempoMaxBpmAction) {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).push_first(ctx, action);
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<TempoMaxBpmAction> {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).take(ctx)
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &TempoMaxBpmModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let body = crate::layout::body_rect(rect, model.titlebar);
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
    input_row(ui, inner, model);
    label_block(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left(), inner.top() + HINT_TOP),
            Vec2::new(inner.width(), HINT_H),
        ),
        &model.hint,
        12.0,
        theme::text_muted(),
    );
    if let Some(error) = &model.error {
        label_block(
            ui,
            Rect::from_min_size(
                egui::pos2(inner.left(), inner.top() + ERROR_TOP),
                Vec2::new(inner.width(), ERROR_H),
            ),
            error,
            12.0,
            theme::danger(),
        );
    }
    buttons(ui, inner, model);
}

/// 输入行：标签（14px）+ 12px + 120×33 输入框。
fn input_row(ui: &mut egui::Ui, inner: Rect, model: &TempoMaxBpmModel) {
    let mut buffer = model.input.clone();
    let row_y = inner.top() + INPUT_TOP;
    // 标签宽度先量出来，输入框才能精确落在「标签 + 12」处。
    let label_width = ui
        .painter()
        .layout_no_wrap(
            model.label.clone(),
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
        RichText::new(&model.label).size(14.0).color(theme::text()),
    ));

    // 外框手绘：`TextEdit` 的行高会把 `Frame` 撑高，这里按固定盒子摆放。
    let input_rect = Rect::from_min_size(
        egui::pos2(inner.left() + label_width + 12.0, row_y),
        Vec2::new(INPUT_W, INPUT_H),
    );
    ui.painter().rect(
        input_rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );
    let mut text_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(input_rect.shrink2(Vec2::new(10.0, 6.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = text_ui
        .push_id("tempo-max-bpm-input", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .font(FontId::proportional(14.0))
                    .text_color(theme::text())
                    .desired_width(INPUT_W - 22.0),
            )
        })
        .inner;
    if response.changed() {
        push_action(ui.ctx(), TempoMaxBpmAction::Input(buffer));
    }
}

/// 按钮：确定（主色）+ 12 + 取消（次要）。
fn buttons(ui: &mut egui::Ui, inner: Rect, model: &TempoMaxBpmModel) {
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
        push_action(ui.ctx(), TempoMaxBpmAction::Confirm);
    }
    row.add_space(BUTTON_GAP);
    if button(&mut row, &model.cancel, theme::toolbar_bg(), theme::text()).clicked() {
        push_action(ui.ctx(), TempoMaxBpmAction::Cancel);
    }
}

/// 固定矩形内的左对齐文本标签（保留可访问树节点）。
fn label_block(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .selectable(false)
            .truncate(),
    );
}

fn button(ui: &mut egui::Ui, label: &str, fill: Color32, text: Color32) -> egui::Response {
    let button = crate::widgets::text_button(RichText::new(label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    // 生产按钮 `padding([8,24])`：宽度随文本，高度固定 34。
    let width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(14.0), text)
        .size()
        .x
        .ceil()
        + 48.0;
    ui.add_sized(Vec2::new(width, BUTTON_H), button)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> TempoMaxBpmModel {
        TempoMaxBpmModel {
            titlebar: false,
            title: "自定义 BPM 上限".into(),
            label: "Tempo BPM 上限:".into(),
            input: "512".into(),
            hint: "上限越大，Tempo 曲线越平坦".into(),
            error: None,
            ok: "确定".into(),
            cancel: "取消".into(),
        }
    }

    #[test]
    fn test_model_carries_input_and_error() {
        let mut m = model();
        assert_eq!(m.input, "512");
        assert!(m.error.is_none());
        m.error = Some("请输入大于 0 的数字".into());
        assert!(m.error.as_deref().is_some_and(|e| e.contains("大于 0")));
    }

    #[test]
    fn test_action_channel_keeps_first_action_per_frame() {
        let ctx = egui::Context::default();
        push_action(&ctx, TempoMaxBpmAction::Input("1024".into()));
        push_action(&ctx, TempoMaxBpmAction::Confirm);
        assert_eq!(
            take_action(&ctx),
            Some(TempoMaxBpmAction::Input("1024".into())),
            "同一帧只保留第一个动作"
        );
        assert_eq!(take_action(&ctx), None, "取走后不得残留");
    }
}
