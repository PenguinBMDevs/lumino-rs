//! 云浏览面板的控件原语：按钮、输入框、固定矩形文本。
//!
//! 全部按固定矩形绘制（`ui.put` / `new_child`），不做自动布局——面板与列表的
//! 坐标由调用方算好，这里只负责外观，保证与 上一代 的像素口径一致。

use crate::theme;
use egui::{Align, Color32, CornerRadius, FontId, RichText, Stroke, Vec2};

/// 主色按钮（`padding([6,20])`）。
pub(super) fn button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    size: f32,
    primary: bool,
) -> egui::Response {
    let (fill, text) = if primary {
        (theme::accent(), theme::on_accent())
    } else {
        (theme::toolbar_bg(), theme::text())
    };
    let button = crate::widgets::text_button(RichText::new(label).size(size).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

/// 工具栏按钮：上一代 默认 `button` 是**主色底**；禁用态用 `primary.weak` 底。
pub(super) fn small_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    size: f32,
    enabled: bool,
) -> egui::Response {
    // 上一代 的禁用态 = 主色 `scale_alpha(0.5)` 叠在底色上（实测 ≈ 0.33 不透明）。
    let (fill, text) = if enabled {
        (theme::accent(), theme::on_accent())
    } else {
        let fill = mix(theme::base_bg(), theme::accent(), 0.285);
        (fill, mix(fill, Color32::WHITE, 0.55))
    };
    let button = crate::widgets::text_button(RichText::new(label).size(size).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

/// 行尾小按钮（12px、`padding([2,8])`、无底色）。
pub(super) fn tiny_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let text = if enabled {
        theme::text()
    } else {
        theme::text_muted()
    };
    let button = crate::widgets::text_button(RichText::new(label).size(12.0).color(text))
        .frame(false)
        .corner_radius(CornerRadius::same(3));
    ui.put(rect, button)
}

/// 固定盒子输入框。
pub(super) fn input(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &'static str,
    value: &str,
    placeholder: &str,
    changed: impl FnOnce(String),
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
            .max_rect(rect.shrink2(Vec2::new(8.0, 5.0)))
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("cloud-input", id), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::proportional(theme::font::LABEL))
                    .text_color(theme::text())
                    .hint_text(
                        RichText::new(placeholder)
                            .size(theme::font::LABEL)
                            .color(theme::text_dim()),
                    )
                    .desired_width(rect.width() - 18.0),
            )
        })
        .inner;
    if response.changed() {
        changed(buffer);
    }
}

/// 两色按 `t` 线性混合（`t = 0` → `a`，`t = 1` → `b`）。
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// 固定矩形内的左对齐文本（保留可访问树节点）。
///
/// 超出矩形宽度时由 egui 以「…」省略，绝不换行——文件名 / 路径过长时
/// 只能压缩自己，不能压到下一行或行尾按钮上。`Label` 默认在省略态悬停
/// 显示全文，无需额外 tooltip。
pub(super) fn label_left(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    text: &str,
    size: f32,
    color: Color32,
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .selectable(false)
            .truncate(),
    );
}

/// 居中文本（进入可访问树；超宽同样省略并悬停显示全文）。
pub(super) fn label_center(
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
