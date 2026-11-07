//! 小型控件工厂：把生产 iced 的按钮 / 图标 / 文本尺寸契约搬到 egui。

use egui::{
    Align2, Color32, CornerRadius, FontId, Layout, Response, RichText, Sense, Stroke, Vec2,
};

use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;

/// 指定尺寸的纯文本。
pub fn label(ui: &mut egui::Ui, text: impl Into<String>, size: f32, color: Color32) {
    ui.add(egui::Label::new(RichText::new(text.into()).size(size).color(color)).selectable(false));
}

/// 可点击的菜单文本（标题栏菜单 / 设置菜单）。
pub fn menu_label(ui: &mut egui::Ui, text: &str, selected: bool) -> Response {
    let color = if selected { theme::ACCENT } else { theme::TEXT };
    ui.add(
        egui::Label::new(RichText::new(text).size(theme::font::BODY).color(color))
            .selectable(false)
            .sense(Sense::click()),
    )
}

/// 路由栏按钮：48×48 命中区；`indicator` 为 Some 时点亮 2px 指示条。
pub fn route_button(
    ui: &mut egui::Ui,
    cache: &mut IconCache,
    icon: Icon,
    indicator: Option<Color32>,
    indicator_on_right: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(layout::ROUTE_BAR_W), Sense::click());
    if let Some(color) = indicator {
        let x = if indicator_on_right {
            rect.right() - 2.0
        } else {
            rect.left()
        };
        ui.painter().rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(2.0, rect.height())),
            0,
            color,
        );
    }
    let image = cache.default_image(ui.ctx(), icon, 20.0);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
    );
    child.add(egui::Button::image(image).frame(false));
    response
}

/// 图标按钮：固定外框尺寸，图标按生产尺寸绘制。
///
/// 注意：egui 0.37 的 `Button::fill/stroke` 会重新打开 frame，
/// 因此选中态用 `stroke(Stroke::NONE)` 而不是 `frame(false)`。
pub fn icon_button(
    ui: &mut egui::Ui,
    cache: &mut IconCache,
    icon: Icon,
    icon_size: f32,
    button_size: Vec2,
    selected: bool,
) -> Response {
    let image = cache.default_image(ui.ctx(), icon, icon_size);
    let button = if selected {
        egui::Button::image(image)
            .fill(theme::SELECTED_BG)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(4))
    } else {
        egui::Button::image(image).frame(false)
    };
    ui.add_sized(button_size, button)
}

/// 标题栏窗口按钮（45×29，图标 10）。
pub fn traffic_button(
    ui: &mut egui::Ui,
    cache: &mut IconCache,
    icon: Icon,
    close: bool,
) -> Response {
    let image = cache.default_image(ui.ctx(), icon, layout::TRAFFIC_ICON);
    let button = egui::Button::image(image).frame(false);
    let response = ui.add_sized([layout::TRAFFIC_BTN_W, layout::TRAFFIC_BTN_H], button);
    if response.hovered() {
        ui.painter().rect_filled(
            response.rect,
            0,
            if close {
                theme::CLOSE_HOVER
            } else {
                theme::SELECTED_BG
            },
        );
    }
    response
}

/// 强调色主按钮（确认 / 保存）。
pub fn accent_button(ui: &mut egui::Ui, text: &str, size: Vec2) -> Response {
    let button = egui::Button::new(
        RichText::new(text)
            .size(theme::font::LABEL)
            .color(Color32::from_rgb(0x12, 0x1A, 0x2A)),
    )
    .fill(theme::ACCENT)
    .corner_radius(CornerRadius::same(4));
    ui.add_sized(size, button)
}

/// 次级按钮（关闭 / 取消）。
pub fn secondary_button(ui: &mut egui::Ui, text: &str, size: Vec2) -> Response {
    let button = egui::Button::new(
        RichText::new(text)
            .size(theme::font::LABEL)
            .color(theme::TEXT),
    )
    .fill(theme::TOOLBAR_BG)
    .corner_radius(CornerRadius::same(4));
    ui.add_sized(size, button)
}

/// 工具栏小文本按钮（精度 / 数值）。
pub fn small_text_button(ui: &mut egui::Ui, text: &str, selected: bool) -> Response {
    let fill = if selected {
        theme::SELECTED_BG
    } else {
        theme::TOOLBAR_BG
    };
    let button = egui::Button::new(
        RichText::new(text)
            .size(theme::font::SMALL)
            .color(theme::TEXT),
    )
    .fill(fill)
    .corner_radius(CornerRadius::same(3));
    ui.add_sized([44.0, 26.0], button)
}

/// 下拉框（设置页 / 工具栏）。
pub fn combo_box(
    ui: &mut egui::Ui,
    id: &str,
    selected: &str,
    options: &[&str],
    width: f32,
) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(
            RichText::new(selected)
                .size(theme::font::BODY)
                .color(theme::TEXT),
        )
        .width(width)
        .show_ui(ui, |ui| {
            for option in options {
                if ui
                    .selectable_label(
                        *option == selected,
                        RichText::new(*option).size(theme::font::BODY),
                    )
                    .clicked()
                {
                    changed = true;
                }
            }
        });
    changed
}

/// 单行数字输入框。
pub fn numeric_input(ui: &mut egui::Ui, value: &mut String, width: f32) -> Response {
    ui.add_sized(
        [width, 26.0],
        egui::TextEdit::singleline(value)
            .font(FontId::proportional(theme::font::BODY))
            .text_color(theme::TEXT)
            .align(Align2::CENTER_CENTER),
    )
}

/// 固定尺寸的空白占位。
pub fn spacer(ui: &mut egui::Ui, width: f32) {
    ui.add_space(width);
}

/// 在固定矩形内居中绘制图标。
pub fn centered_icon(
    ui: &mut egui::Ui,
    cache: &mut IconCache,
    icon: Icon,
    size: f32,
    rect: egui::Rect,
) {
    let image = cache.default_image(ui.ctx(), icon, size);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
    );
    child.add(image);
}
