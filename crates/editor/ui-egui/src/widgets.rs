//! 小型控件工厂：把生产 iced 的按钮 / 图标 / 文本尺寸契约搬到 egui。

use egui::{
    Align, Color32, CornerRadius, FontId, Layout, Rect, Response, RichText, Sense, Stroke, Vec2,
};

use crate::chrome::SettingsPalette;
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;

/// 指定尺寸的纯文本。
pub fn label(ui: &mut egui::Ui, text: impl Into<String>, size: f32, color: Color32) {
    ui.add(egui::Label::new(RichText::new(text.into()).size(size).color(color)).selectable(false));
}

/// 可点击的菜单文本（标题栏菜单 / 设置菜单）。
pub fn menu_label(ui: &mut egui::Ui, text: &str, selected: bool) -> Response {
    menu_label_sized(ui, text, selected, theme::font::BODY)
}

/// 指定字号的菜单文本。
pub fn menu_label_sized(ui: &mut egui::Ui, text: &str, selected: bool, size: f32) -> Response {
    let color = if selected {
        theme::accent()
    } else {
        theme::text()
    };
    ui.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
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
    // 自绘而不是 `egui::Button`：`Button` 会把 `spacing.interact_size`（40×18）
    // 和 frame 内边距算进最小尺寸，导致「请求 25px 实际占 42px」——相邻工具按钮
    // 命中区互相重叠（真机表现：点第 2 个选择器却选中第 1 个）。
    // 这里按请求尺寸精确分配并手绘，几何与生产（iced）逐像素一致。
    let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), theme::selected_bg());
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), theme::hover_bg());
    }
    let image = cache.default_image(ui.ctx(), icon, icon_size);
    image.paint_at(
        ui,
        Rect::from_center_size(rect.center(), Vec2::splat(icon_size)),
    );
    response
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
    // 纯图标按钮必须提供可访问名，否则无障碍树里是无名按钮。
    let label = match icon {
        Icon::WindowClose => "关闭窗口",
        Icon::WindowMax => "最大化窗口",
        Icon::WindowMin => "最小化窗口",
        _ => "窗口按钮",
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    if response.hovered() {
        ui.painter().rect_filled(
            response.rect,
            0,
            if close {
                theme::close_hover()
            } else {
                theme::selected_bg()
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
            .color(theme::on_accent()),
    )
    .fill(theme::accent())
    .corner_radius(CornerRadius::same(4));
    ui.add_sized(size, button)
}

/// 次级按钮（关闭 / 取消）。
pub fn secondary_button(ui: &mut egui::Ui, text: &str, size: Vec2) -> Response {
    let button = egui::Button::new(
        RichText::new(text)
            .size(theme::font::LABEL)
            .color(theme::text()),
    )
    .fill(theme::toolbar_bg())
    .corner_radius(CornerRadius::same(4));
    ui.add_sized(size, button)
}

/// 工具栏小文本按钮（精度 / 数值）。
pub fn small_text_button(ui: &mut egui::Ui, text: &str, selected: bool) -> Response {
    let fill = if selected {
        theme::selected_bg()
    } else {
        theme::toolbar_bg()
    };
    let button = egui::Button::new(
        RichText::new(text)
            .size(theme::font::SMALL)
            .color(theme::text()),
    )
    .fill(fill)
    .corner_radius(CornerRadius::same(3));
    ui.add_sized([44.0, 26.0], button)
}

/// 设置页下拉框字号（iced `pick_list` 默认 16）。
const SETTINGS_COMBO_TEXT: f32 = 16.0;
/// 设置页输入框字号（iced `text_input` 默认 16）。
const SETTINGS_INPUT_TEXT: f32 = 16.0;

/// 下拉框（设置页 / 工具栏）。
pub fn combo_box(
    ui: &mut egui::Ui,
    id: &str,
    selected: &str,
    options: &[String],
    width: f32,
    palette: &SettingsPalette,
) -> Option<usize> {
    let mut selected_index = None;
    // 生产 `pick_list` 高 32、左右内边距 8、边框 `background.strong`；
    // egui 默认按 `interact_size.y`(18) + 文本只有 24 高，必须显式对齐。
    let previous_padding = ui.spacing().button_padding;
    let previous_interact = ui.spacing().interact_size;
    ui.spacing_mut().button_padding.x = 8.0;
    ui.spacing_mut().interact_size.y = layout::SETTINGS_COMBO_H;
    // 生产 `pick_list` 按钮底色 = `background.weak`、文字 = `background.base.text`；
    // egui 默认按钮底色是中性灰，深浅主题都必须显式对齐。
    let previous_visuals = ui.visuals().clone();
    ui.visuals_mut().widgets.inactive.weak_bg_fill = palette.toolbar;
    ui.visuals_mut().widgets.inactive.bg_fill = palette.toolbar;
    ui.visuals_mut().widgets.inactive.fg_stroke.color = palette.text;
    let response = egui::ComboBox::from_id_salt(id)
        .selected_text(
            RichText::new(selected)
                .size(SETTINGS_COMBO_TEXT)
                .color(palette.text),
        )
        .width(width)
        .show_ui(ui, |ui| {
            for (index, option) in options.iter().enumerate() {
                if ui
                    .selectable_label(
                        option.as_str() == selected,
                        RichText::new(option.as_str()).size(SETTINGS_COMBO_TEXT),
                    )
                    .clicked()
                {
                    selected_index = Some(index);
                }
            }
        });
    ui.spacing_mut().button_padding = previous_padding;
    ui.spacing_mut().interact_size = previous_interact;
    *ui.visuals_mut() = previous_visuals;
    let rect = response.response.rect;
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(4),
        Stroke::new(1.0, palette.input_border),
        egui::StrokeKind::Inside,
    );
    selected_index
}

/// 单行数字输入框：外观对齐生产 `text_input`（高 31、1px `background.strong`
/// 边框、背景 `background.base`、左内边距 8、字号 16）。
pub fn numeric_input(
    ui: &mut egui::Ui,
    value: &mut String,
    width: f32,
    palette: &SettingsPalette,
) -> Response {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(width, layout::SETTINGS_INPUT_H), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        palette.background,
        Stroke::new(1.0, palette.input_border),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.add(
        egui::TextEdit::singleline(value)
            .frame(egui::Frame::NONE)
            .font(FontId::proportional(SETTINGS_INPUT_TEXT))
            .text_color(palette.text)
            .desired_width(f32::INFINITY),
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
