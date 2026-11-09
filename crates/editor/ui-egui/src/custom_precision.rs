//! 自定义精度对话框（`DialogType::CustomPrecision`）。
//!
//! 几何对齐生产 `view/custom_precision_dialog.rs`：整窗 base 底、内边距 24；
//! 左列 320 宽（三连音数 50×33 + 8 + 分音符 50×33 + 8 + 「分音符」14px，
//! 12 → 附点行（标签 13px + 4 个 26 高分段按钮），12 → 「除以」14px + 50 +
//! 除数 50×33），右列 100 宽按钮（确定 / 取消，34 高，间距 12），两列间距 20、
//! 右列相对左列垂直居中。错误行（11px 危险色）出现在左列底部。
//!
//! 输入只回传原样文本；数值范围与提交校验由生产侧负责。

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, RichText, Stroke, Vec2,
};

use crate::chrome::CustomPrecisionModel;
use crate::theme;

const ACTION_ID: &str = "lumino.custom-precision.action";
/// 整窗内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 左列宽度（生产 `Length::Fixed(320)`）。
const LEFT_W: f32 = 320.0;
/// 行内水平间距（生产 `space().width(8)`）。
const GAP: f32 = 8.0;
/// 行间距（生产 `space().height(12)`）。
const ROW_GAP: f32 = 12.0;
/// 输入框（生产 `text_input.padding([6,10])`、固定 50 宽）。
const INPUT_W: f32 = 50.0;
const INPUT_H: f32 = 33.0;
/// 附点分段按钮高度（生产 `padding([4,7])` + 11px 文本 + 1px 边框）。
const DOT_H: f32 = 26.0;
const DOT_GAP: f32 = 6.0;
/// 右列按钮（生产固定 100 宽、`padding([8,32])`）。
const BUTTON_W: f32 = 100.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;
/// 两列间距（生产 `space().width(20)`）。
const COLUMN_GAP: f32 = 20.0;
/// 错误行高（11px 文本）。
const ERROR_H: f32 = 15.0;

/// 自定义精度对话框产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomPrecisionAction {
    /// 三连音数变化。
    TupletCountChanged(String),
    /// 分音符值变化。
    NoteValueChanged(String),
    /// 附点类型切换。
    DotTypeChanged(lumino_core::DotType),
    /// 除数变化。
    DivisorChanged(String),
    /// 确定：生产侧校验并提交。
    Confirm,
    /// 取消：关闭对话框。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: CustomPrecisionAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<CustomPrecisionAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<CustomPrecisionAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<CustomPrecisionAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &CustomPrecisionModel) {
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

    let left_h = left_height(model);
    // 左列：三行输入区。
    first_row(ui, inner, model);
    let dot_y = inner.top() + INPUT_H + ROW_GAP;
    dot_row(ui, egui::pos2(inner.left(), dot_y), model);
    let divide_y = dot_y + DOT_H + ROW_GAP;
    divide_row(ui, egui::pos2(inner.left(), divide_y), model);
    if let Some(error) = &model.error {
        let error_rect = Rect::from_min_size(
            egui::pos2(inner.left(), divide_y + INPUT_H + 6.0),
            Vec2::new(LEFT_W, ERROR_H),
        );
        label_block(ui, error_rect, error, 11.0, theme::danger());
    }

    // 右列：确定 / 取消，相对左列垂直居中。
    let buttons_top = inner.top() + (left_h - (BUTTON_H * 2.0 + BUTTON_GAP)).max(0.0) / 2.0;
    let buttons_x = inner.left() + LEFT_W + COLUMN_GAP;
    if button(
        ui,
        Rect::from_min_size(
            egui::pos2(buttons_x, buttons_top),
            Vec2::new(BUTTON_W, BUTTON_H),
        ),
        &model.ok,
        theme::accent(),
        theme::on_accent(),
    )
    .clicked()
    {
        push_action(ui.ctx(), CustomPrecisionAction::Confirm);
    }
    if button(
        ui,
        Rect::from_min_size(
            egui::pos2(buttons_x, buttons_top + BUTTON_H + BUTTON_GAP),
            Vec2::new(BUTTON_W, BUTTON_H),
        ),
        &model.cancel,
        theme::toolbar_bg(),
        theme::text(),
    )
    .clicked()
    {
        push_action(ui.ctx(), CustomPrecisionAction::Cancel);
    }
}

/// 左列总高（含可选错误行）。
fn left_height(model: &CustomPrecisionModel) -> f32 {
    let base = INPUT_H + ROW_GAP + DOT_H + ROW_GAP + INPUT_H;
    if model.error.is_some() {
        base + 6.0 + ERROR_H
    } else {
        base
    }
}

/// 第一行：三连音数 + 8 + 分音符值 + 8 + 「分音符」。
fn first_row(ui: &mut egui::Ui, inner: Rect, model: &CustomPrecisionModel) {
    let y = inner.top();
    input(
        ui,
        Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(INPUT_W, INPUT_H)),
        "tuplet-count",
        &model.tuplet_count,
        CustomPrecisionAction::TupletCountChanged,
    );
    input(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left() + INPUT_W + GAP, y),
            Vec2::new(INPUT_W, INPUT_H),
        ),
        "note-value",
        &model.note_value,
        CustomPrecisionAction::NoteValueChanged,
    );
    label_block(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left() + (INPUT_W + GAP) * 2.0, y),
            Vec2::new(LEFT_W - (INPUT_W + GAP) * 2.0, INPUT_H),
        ),
        &model.note_label,
        14.0,
        theme::text(),
    );
}

/// 附点行：13px 标签 + 6 + 四个分段按钮（选中 `primary.weak` 底）。
fn dot_row(ui: &mut egui::Ui, origin: egui::Pos2, model: &CustomPrecisionModel) {
    let width = ui
        .painter()
        .layout_no_wrap(
            model.dot_label.clone(),
            FontId::proportional(13.0),
            theme::text(),
        )
        .size()
        .x
        .ceil();
    label_block(
        ui,
        Rect::from_min_size(origin, Vec2::new(width, DOT_H)),
        &model.dot_label,
        13.0,
        theme::text(),
    );
    let mut x = origin.x + width + DOT_GAP;
    for option in &model.dots {
        let text_width = ui
            .painter()
            .layout_no_wrap(
                option.label.clone(),
                FontId::proportional(11.0),
                theme::text(),
            )
            .size()
            .x
            .ceil();
        let button_w = text_width + 14.0 + 2.0;
        let rect = Rect::from_min_size(
            egui::pos2(x, origin.y + (DOT_H - 26.0) / 2.0),
            Vec2::new(button_w, 26.0),
        );
        let response = ui.interact(
            rect,
            Id::unique(("custom-precision-dot", option.dot as u8)),
            egui::Sense::click(),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &option.label));
        let fill = if option.selected {
            theme::accent_dark()
        } else if response.hovered() {
            theme::selected_bg()
        } else {
            theme::toolbar_bg()
        };
        let text_color = if option.selected {
            theme::text_on_weak()
        } else {
            theme::text()
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(4),
            fill,
            Stroke::new(1.0, theme::selected_bg()),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &option.label,
            FontId::proportional(11.0),
            text_color,
        );
        if response.clicked() {
            push_action(ui.ctx(), CustomPrecisionAction::DotTypeChanged(option.dot));
        }
        x += button_w + DOT_GAP;
    }
}

/// 「除以」行：标签 + 50 + 除数输入框。
fn divide_row(ui: &mut egui::Ui, origin: egui::Pos2, model: &CustomPrecisionModel) {
    let width = ui
        .painter()
        .layout_no_wrap(
            model.divide_label.clone(),
            FontId::proportional(14.0),
            theme::text(),
        )
        .size()
        .x
        .ceil();
    label_block(
        ui,
        Rect::from_min_size(origin, Vec2::new(width, INPUT_H)),
        &model.divide_label,
        14.0,
        theme::text(),
    );
    input(
        ui,
        Rect::from_min_size(
            egui::pos2(origin.x + width + 50.0, origin.y),
            Vec2::new(INPUT_W, INPUT_H),
        ),
        "divisor",
        &model.divisor,
        CustomPrecisionAction::DivisorChanged,
    );
}

/// 固定盒子输入框：base 底 + 1px `strong` 边框、圆角 4，文本内缩 10/6。
fn input(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &'static str,
    value: &str,
    action: impl FnOnce(String) -> CustomPrecisionAction,
) {
    let mut buffer = value.to_owned();
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
        .push_id(("custom-precision", id), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .font(FontId::proportional(14.0))
                    .text_color(theme::text())
                    .desired_width(rect.width() - 22.0),
            )
        })
        .inner;
    if response.changed() {
        push_action(ui.ctx(), action(buffer));
    }
}

/// 固定矩形内的左对齐文本标签（保留可访问树节点）。
fn label_block(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}

fn button(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    fill: Color32,
    text: Color32,
) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}
