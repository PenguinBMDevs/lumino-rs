//! 设置对话框草稿：左侧菜单 + 右侧表单。
//!
//! 布局对齐生产 iced `ui-settings`：
//! - 内容区外边距 16，菜单 200 宽、卡片自适应，菜单与卡片间距 16；
//! - 菜单项行距 52，选中项只改文字颜色（无底色）；
//! - 卡片内边距 20，标题 18px，表单行距 72；
//! - 「确认」按钮在卡片之外、右下角 100×34。

use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Vec2};
use egui_extras::{Column, TableBuilder};

use crate::chrome::SettingsPalette;
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;
use crate::widgets;

/// 设置草稿状态。
pub struct SettingsState {
    /// 图标缓存（必须跨帧存活，否则纹理会在渲染前被释放）。
    pub icons: IconCache,
    /// 当前菜单索引。
    pub selected: usize,
    /// 橡皮擦行为下拉值。
    pub eraser: String,
    /// 添加音轨行为下拉值。
    pub track_add: String,
    /// 日志保留份数。
    pub log_retention: String,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            icons: IconCache::default(),
            selected: 0,
            eraser: "默认 (Shift+拖动框选)".to_owned(),
            track_add: "自动跳转到新音轨".to_owned(),
            log_retention: "10".to_owned(),
        }
    }
}

const MENU_ITEMS: &[(&str, Icon)] = &[
    ("常规", Icon::Gear),
    ("音频", Icon::WaveForm),
    ("界面", Icon::FolderTree),
    ("快捷键", Icon::Clock),
    ("洋葱皮", Icon::Eye),
    ("调色板", Icon::Arrangement),
    ("编辑", Icon::Pencil),
    ("云管理", Icon::Download),
    ("兼容性", Icon::Ban),
    ("关于", Icon::LogoInApp),
];

/// 菜单项行距（iced 基线：52）。
const MENU_ROW_H: f32 = 52.0;

/// 渲染设置对话框。
pub fn show(ui: &mut egui::Ui, state: &mut SettingsState) {
    show_with(ui, state, &SettingsPalette::DARK);
}

/// 按调色板渲染设置对话框（真实窗口叠加层与场景共用）。
pub fn show_with(ui: &mut egui::Ui, state: &mut SettingsState, palette: &SettingsPalette) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    ui.spacing_mut().item_spacing = Vec2::ZERO;

    egui::Panel::top("settings-titlebar")
        .exact_size(layout::TITLEBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(crate::chrome::panel_frame(palette.titlebar))
        .show(ui, |ui| titlebar(ui, palette, &mut state.icons));

    egui::CentralPanel::default()
        .frame(crate::chrome::panel_frame(palette.background))
        .show(ui, |ui| content(ui, state, palette));
}

fn titlebar(ui: &mut egui::Ui, palette: &SettingsPalette, cache: &mut IconCache) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, palette.titlebar);
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        widgets::traffic_button(ui, cache, Icon::WindowClose, true);
        widgets::traffic_button(ui, cache, Icon::WindowMax, false);
        widgets::traffic_button(ui, cache, Icon::WindowMin, false);
    });
}

fn content(ui: &mut egui::Ui, state: &mut SettingsState, palette: &SettingsPalette) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, palette.background);

    const MARGIN: f32 = 16.0;
    const BUTTON_W: f32 = 100.0;
    const BUTTON_H: f32 = 34.0;
    const BUTTON_GAP: f32 = 16.0;

    let inner = rect.shrink(MARGIN);
    let confirm_rect = Rect::from_min_size(
        egui::pos2(inner.right() - BUTTON_W, rect.bottom() - MARGIN - BUTTON_H),
        Vec2::new(BUTTON_W, BUTTON_H),
    );
    let container = Rect::from_min_max(
        inner.min,
        egui::pos2(inner.right(), confirm_rect.top() - BUTTON_GAP),
    );
    // 生产 iced：容器自身 PANEL_BG、内边距 20，菜单与卡片都在内边距之内。
    const CONTAINER_PAD: f32 = 20.0;
    let inner_container = container.shrink(CONTAINER_PAD);
    let menu_rect = Rect::from_min_size(
        inner_container.min,
        Vec2::new(layout::SETTINGS_MENU_W, inner_container.height()),
    );
    let card_rect = Rect::from_min_max(
        egui::pos2(menu_rect.right() + 16.0, inner_container.top()),
        inner_container.max,
    );

    ui.painter()
        .rect_filled(container, CornerRadius::same(16), palette.panel);
    ui.painter()
        .rect_filled(menu_rect, CornerRadius::same(16), palette.toolbar);
    ui.painter().rect_stroke(
        menu_rect,
        CornerRadius::same(16),
        Stroke::new(1.0, palette.border),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .rect_filled(card_rect, CornerRadius::same(16), palette.card);
    ui.painter().rect_stroke(
        card_rect,
        CornerRadius::same(16),
        Stroke::new(1.0, palette.border),
        egui::StrokeKind::Inside,
    );

    menu(ui, menu_rect, state, palette);
    form(ui, card_rect, state, palette);
    confirm(ui, confirm_rect, palette);
}

fn menu(ui: &mut egui::Ui, rect: Rect, state: &mut SettingsState, palette: &SettingsPalette) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(1.0, 1.0)))
            .layout(Layout::top_down(Align::Min)),
    );

    TableBuilder::new(&mut child)
        .id_salt("settings-menu-table")
        .striped(false)
        .column(Column::remainder())
        .body(|body| {
            body.rows(MENU_ROW_H, MENU_ITEMS.len(), |mut row| {
                let index = row.index();
                row.col(|ui| {
                    let (label, icon) = MENU_ITEMS[index];
                    menu_row(ui, state, index, label, icon, palette);
                });
            });
        });
}

fn menu_row(
    ui: &mut egui::Ui,
    state: &mut SettingsState,
    index: usize,
    label: &str,
    icon: Icon,
    palette: &SettingsPalette,
) {
    let selected = state.selected == index;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), MENU_ROW_H), Sense::click());
    if response.clicked() {
        state.selected = index;
    }

    // 生产 iced：菜单容器内边距 8 + 菜单项水平内边距 16 → 内容起点 +24；
    // 图标容器 24 宽（图标 18 居中）、间距 8、箭头 12px 右对齐（右内边距 24）。
    let content_left = rect.left() + 26.0;
    let icon_rect = Rect::from_center_size(
        egui::pos2(content_left + 12.0, rect.center().y),
        Vec2::splat(18.0),
    );
    widgets::centered_icon(ui, &mut state.icons, icon, 18.0, icon_rect);
    ui.painter().text(
        egui::pos2(content_left + 32.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme::font::LABEL),
        if selected {
            palette.accent_strong
        } else {
            palette.text
        },
    );
    ui.painter().text(
        egui::pos2(rect.right() - 26.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        ">",
        egui::FontId::proportional(theme::font::SMALL + 1.0),
        theme::TEXT_DIM,
    );
}

fn form(ui: &mut egui::Ui, rect: Rect, state: &mut SettingsState, palette: &SettingsPalette) {
    let pad = layout::SETTINGS_PADDING;
    let left = rect.left() + pad;
    let base = rect.top();

    ui.painter().text(
        egui::pos2(left, base + 30.0),
        egui::Align2::LEFT_CENTER,
        "常规",
        egui::FontId::proportional(theme::font::TITLE),
        palette.text,
    );

    // 表单：标签 / 控件两列，第一列宽度固定（行位置相对卡片顶部，与 iced 一致）。
    let label_x = left;
    let field_x = left + 104.0;
    form_row(
        ui,
        state,
        label_x,
        field_x,
        base + 99.0,
        "橡皮擦行为:",
        0,
        palette,
    );
    form_row(
        ui,
        state,
        label_x,
        field_x,
        base + 171.0,
        "添加音轨时:",
        1,
        palette,
    );

    ui.painter().text(
        egui::pos2(left, base + 236.0),
        egui::Align2::LEFT_CENTER,
        "日志",
        egui::FontId::proportional(theme::font::SECTION),
        palette.text,
    );

    let log_label = "日志文件保留份数:";
    let log_label_width = ui
        .painter()
        .layout_no_wrap(
            log_label.to_owned(),
            egui::FontId::proportional(theme::font::LABEL),
            palette.text,
        )
        .size()
        .x;
    ui.painter().text(
        egui::pos2(label_x, base + 291.0),
        egui::Align2::LEFT_CENTER,
        log_label,
        egui::FontId::proportional(theme::font::LABEL),
        palette.text,
    );
    // 生产 iced 的行布局：`row![label, space(16), input]`，行间距 8 →
    // 标签与输入框之间实际 32px（8+16+8）。
    let input_rect = Rect::from_min_size(
        egui::pos2(
            label_x + log_label_width + 32.0,
            base + 291.0 - layout::SETTINGS_INPUT_H / 2.0,
        ),
        Vec2::new(80.0, layout::SETTINGS_INPUT_H),
    );
    let mut input_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(input_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    widgets::numeric_input(&mut input_ui, &mut state.log_retention, 80.0, palette);
}

#[allow(clippy::too_many_arguments)]
fn form_row(
    ui: &mut egui::Ui,
    state: &mut SettingsState,
    label_x: f32,
    field_x: f32,
    center_y: f32,
    label: &str,
    index: usize,
    palette: &SettingsPalette,
) {
    ui.painter().text(
        egui::pos2(label_x, center_y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme::font::LABEL),
        palette.text,
    );

    let combo_rect =
        Rect::from_min_size(egui::pos2(field_x, center_y - 16.0), Vec2::new(200.0, 32.0));
    let mut combo_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(combo_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    let (value, options): (&str, [&str; 2]) = if index == 0 {
        (&state.eraser, ["默认 (Shift+拖动框选)", "直接选择"])
    } else {
        (&state.track_add, ["自动跳转到新音轨", "停留在当前音轨"])
    };
    widgets::combo_box(&mut combo_ui, label, value, &options, 200.0, palette);
}

fn confirm(ui: &mut egui::Ui, rect: Rect, palette: &SettingsPalette) {
    let button = egui::Button::new(
        RichText::new("确认")
            .size(theme::font::LABEL)
            .color(Color32::from_rgb(0x12, 0x1A, 0x2A)),
    )
    .fill(palette.accent)
    .corner_radius(CornerRadius::same(4));
    ui.put(rect, button);
}
