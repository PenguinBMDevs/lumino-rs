//! 云存储连接面板（`DialogType::CloudConnect`）。
//!
//! 几何对齐生产 `view/cloud_dialog.rs`：整窗内容底、内边距 24、列间距 14；
//! 标题 18px 居中；表单 6 行（标签 110 + 8 + 控件填满，行高 34、行距 8）；
//! 连接失败提示 13px 危险色；隐私提示 12px；底部居中的「取消」+「连接」
//! （连接中按钮变灰并显示「连接中...」）。
//!
//! 输入只回传原样文本，连接流程与校验由 runner 负责。

use egui::{Align, Color32, CornerRadius, FontId, Id, Layout, Rect, RichText, Stroke, Vec2};

use crate::chrome::{CloudConnectModel, CloudField, CloudProtocol};
use crate::theme;

const ACTION_ID: &str = "lumino.cloud-connect.action";
/// 内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 标题行高（18px 文本）。
const TITLE_H: f32 = 27.0;
/// 表单行（生产 `text_input.padding([6,10])`、`column.spacing(10)`；
/// 实测 iced 行盒高 38 物理像素 = 30.4 逻辑，行距 10）。
const ROW_H: f32 = 30.5;
const ROW_GAP: f32 = 10.0;
/// 标签列宽度（生产 `text(width(110))`）与标签/控件间距。
const LABEL_W: f32 = 110.0;
const LABEL_GAP: f32 = 8.0;
/// 按钮（生产 `padding([8,20])` / `[8,28])`）。
const BUTTON_H: f32 = 32.0;
const BUTTON_GAP: f32 = 12.0;

/// 云连接面板产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudConnectAction {
    /// 协议切换。
    ProtocolSelected(CloudProtocol),
    /// 表单字段变化（原样文本）。
    FieldChanged(CloudField, String),
    /// 连接。
    Connect,
    /// 取消（关闭面板）。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: CloudConnectAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<CloudConnectAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<CloudConnectAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<CloudConnectAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染面板（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &CloudConnectModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
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

    let mut y = inner.top();
    // 标题：18px，水平居中（生产 `column.align_x(Center)`）。
    centered_text(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left(), y),
            Vec2::new(inner.width(), TITLE_H),
        ),
        &model.title,
        18.0,
        theme::text(),
    );
    // 标题与表单间距：实测 iced 为 11 逻辑像素（14 时整块下移 4 物理像素）。
    y += TITLE_H + 11.0;

    protocol_row(ui, inner, y, model);
    y += ROW_H + ROW_GAP;
    for field in &model.fields {
        field_row(ui, inner, y, field);
        y += ROW_H + ROW_GAP;
    }
    y += 14.0 - ROW_GAP;

    if let Some(error) = &model.error {
        label_row_text(ui, inner, y, error, 13.0, theme::danger());
        y += 18.0 + 14.0;
    } else {
        y += 4.0 + 14.0;
    }

    centered_text(
        ui,
        Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 16.0)),
        &model.privacy,
        12.0,
        theme::text(),
    );
    y += 16.0 + 14.0;

    buttons_row(ui, inner, y, model);
}

/// 协议行：标签 + 下拉（3 个固定选项）。
fn protocol_row(ui: &mut egui::Ui, inner: Rect, y: f32, model: &CloudConnectModel) {
    label_cell(ui, inner, y, &model.protocol_label);
    let control = Rect::from_min_size(
        egui::pos2(inner.left() + LABEL_W + LABEL_GAP, y),
        Vec2::new(inner.width() - LABEL_W - LABEL_GAP, ROW_H),
    );
    let selected = model
        .protocols
        .iter()
        .find(|option| option.selected)
        .map(|option| option.label.as_str())
        .unwrap_or("");
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(control)
            .layout(Layout::left_to_right(Align::Center)),
    );
    // 让下拉按钮填满整行高度（iced `pick_list` 与输入框同高）。
    child.spacing_mut().interact_size.y = ROW_H;
    let mut chosen = None;
    egui::ComboBox::from_id_salt("cloud-protocol")
        .selected_text(RichText::new(selected).size(theme::font::LABEL))
        // 生产 `pick_list` 未设宽度 → 按选项收缩；实测 114 逻辑宽。
        .width(113.0)
        .show_ui(&mut child, |ui| {
            for option in &model.protocols {
                if ui
                    .selectable_label(option.selected, &option.label)
                    .clicked()
                {
                    chosen = Some(option.protocol);
                }
            }
        });
    if let Some(protocol) = chosen {
        push_action(ui.ctx(), CloudConnectAction::ProtocolSelected(protocol));
    }
}

/// 表单行：固定 110 标签 + 8 + 输入框。
fn field_row(ui: &mut egui::Ui, inner: Rect, y: f32, field: &crate::chrome::CloudFieldModel) {
    label_cell(ui, inner, y, &field.label);
    let rect = Rect::from_min_size(
        egui::pos2(inner.left() + LABEL_W + LABEL_GAP, y),
        Vec2::new(inner.width() - LABEL_W - LABEL_GAP, ROW_H),
    );
    let mut buffer = field.value.clone();
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(10.0, 6.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("cloud-field", field.field as u8), |ui| {
            let mut edit = egui::TextEdit::singleline(&mut buffer)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(FontId::proportional(theme::font::LABEL))
                .text_color(theme::text())
                .hint_text(
                    RichText::new(&field.placeholder)
                        .size(theme::font::LABEL)
                        .color(theme::text_dim()),
                )
                .desired_width(rect.width() - 22.0);
            if field.password {
                edit = edit.password(true);
            }
            ui.add(edit)
        })
        .inner;
    if response.changed() {
        push_action(
            ui.ctx(),
            CloudConnectAction::FieldChanged(field.field, buffer),
        );
    }
}

/// 固定 110 宽的标签单元（垂直居中）。
fn label_cell(ui: &mut egui::Ui, inner: Rect, y: f32, text: &str) {
    let rect = Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(LABEL_W, ROW_H));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(
        RichText::new(text)
            .size(theme::font::LABEL)
            .color(theme::text()),
    ));
}

/// 底部：取消（次要）+ 12 + 连接（主色，连接中置灰）。
fn buttons_row(ui: &mut egui::Ui, inner: Rect, y: f32, model: &CloudConnectModel) {
    let cancel_w = text_width(ui, &model.cancel) + 40.0;
    let connect_w = text_width(ui, &model.connect) + 56.0;
    let total = cancel_w + BUTTON_GAP + connect_w;
    let left = inner.center().x - total / 2.0;

    let cancel_rect = Rect::from_min_size(egui::pos2(left, y), Vec2::new(cancel_w, BUTTON_H));
    if button(
        ui,
        cancel_rect,
        &model.cancel,
        theme::toolbar_bg(),
        theme::text(),
    )
    .clicked()
    {
        push_action(ui.ctx(), CloudConnectAction::Cancel);
    }

    let connect_rect = Rect::from_min_size(
        egui::pos2(left + cancel_w + BUTTON_GAP, y),
        Vec2::new(connect_w, BUTTON_H),
    );
    let (fill, text) = if model.connecting {
        (theme::toolbar_bg(), theme::text_muted())
    } else {
        (theme::accent(), theme::on_accent())
    };
    if button(ui, connect_rect, &model.connect, fill, text).clicked() && !model.connecting {
        push_action(ui.ctx(), CloudConnectAction::Connect);
    }
}

fn text_width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            label.to_owned(),
            FontId::proportional(theme::font::LABEL),
            theme::text(),
        )
        .size()
        .x
        .ceil()
}

fn button(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    fill: Color32,
    text: Color32,
) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(theme::font::LABEL).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(6));
    ui.put(rect, button)
}

/// 水平居中的文本标签。
fn centered_text(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}

/// 左对齐的行文本（错误提示）。
fn label_row_text(ui: &mut egui::Ui, inner: Rect, y: f32, text: &str, size: f32, color: Color32) {
    let rect = Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 18.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
