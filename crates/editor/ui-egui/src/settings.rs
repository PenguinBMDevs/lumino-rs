//! 设置对话框草稿：左侧菜单 + 右侧表单。
//!
//! 布局对齐生产 iced `ui-settings`：
//! - 内容区外边距 16，菜单 200 宽、卡片自适应，菜单与卡片间距 16；
//! - 菜单项行距 52，选中项只改文字颜色（无底色）；
//! - 卡片内边距 20，标题 18px，表单行距 72；
//! - 「确认」按钮在卡片之外、右下角 100×34。

use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Vec2};
use egui_extras::{Column, TableBuilder};

use crate::chrome::{SettingsLabels, SettingsPalette};
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
    /// 本地化文案（真实窗口由 `SettingsSnapshot` 注入，场景用中文默认值）。
    pub labels: SettingsLabels,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            icons: IconCache::default(),
            selected: 0,
            eraser: DEFAULT_ERASER.to_owned(),
            track_add: DEFAULT_TRACK_ADD.to_owned(),
            log_retention: "10".to_owned(),
            labels: default_labels(),
        }
    }
}

/// 场景默认的橡皮擦行为文本（与生产 `eraser_behavior_name(ZhCn)` 一致）。
const DEFAULT_ERASER: &str = "默认 (Shift+拖动框选)";
/// 场景默认的添加音轨行为文本（与生产 `track_add_behavior_name(ZhCn)` 一致）。
const DEFAULT_TRACK_ADD: &str = "自动跳转到新音轨";

/// 场景默认文案（中文）：与生产 `settings_translations(ZhCn)` 同值。
fn default_labels() -> SettingsLabels {
    SettingsLabels {
        menu: [
            "常规",
            "音频",
            "界面",
            "快捷键",
            "洋葱皮",
            "调色板",
            "编辑",
            "云管理",
            "兼容性",
            "关于",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        title: "常规".to_owned(),
        eraser_label: "橡皮擦行为:".to_owned(),
        eraser_options: vec![
            "默认 (Shift+拖动框选)".to_owned(),
            "直接框选 (无需Shift)".to_owned(),
        ],
        track_add_label: "添加音轨时:".to_owned(),
        track_add_options: vec!["自动跳转到新音轨".to_owned(), "保持当前音轨".to_owned()],
        log_section: "日志".to_owned(),
        log_label: "日志文件保留份数:".to_owned(),
        log_placeholder: "10".to_owned(),
        confirm: "确认".to_owned(),
    }
}

/// 菜单图标（顺序与 `SettingsLabels::menu` 一一对应）。
const MENU_ICONS: [Icon; 10] = [
    Icon::Gear,
    Icon::WaveForm,
    Icon::FolderTree,
    Icon::Clock,
    Icon::Eye,
    Icon::Arrangement,
    Icon::Pencil,
    Icon::Download,
    Icon::Ban,
    Icon::LogoInApp,
];

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

    const MARGIN: f32 = layout::SETTINGS_MARGIN;
    const BUTTON_W: f32 = layout::SETTINGS_CONFIRM_W;
    const BUTTON_H: f32 = layout::SETTINGS_CONFIRM_H;
    const BUTTON_GAP: f32 = layout::SETTINGS_CONFIRM_GAP;

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
    let confirm_label = state.labels.confirm.clone();
    confirm(ui, confirm_rect, palette, &confirm_label);
}

fn menu(ui: &mut egui::Ui, rect: Rect, state: &mut SettingsState, palette: &SettingsPalette) {
    // 先取一份文案副本：表格闭包需要可变借用 `state`（点击切换页签）。
    let labels = state.labels.menu.clone();
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
            body.rows(layout::SETTINGS_MENU_ROW_H, MENU_ICONS.len(), |mut row| {
                let index = row.index();
                row.col(|ui| {
                    let label = labels.get(index).map_or("", String::as_str);
                    menu_row(ui, state, index, label, MENU_ICONS[index], palette);
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
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), layout::SETTINGS_MENU_ROW_H),
        Sense::click(),
    );
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
        palette.text_dim,
    );
}

fn form(ui: &mut egui::Ui, rect: Rect, state: &mut SettingsState, palette: &SettingsPalette) {
    let pad = layout::SETTINGS_PADDING;
    let left = rect.left() + pad;
    let base = rect.top();

    // 文案副本：下方控件需要可变借用 `state`（输入框）。
    let title = state.labels.title.clone();
    let eraser_label = state.labels.eraser_label.clone();
    let eraser_options = state.labels.eraser_options.clone();
    let track_add_label = state.labels.track_add_label.clone();
    let track_add_options = state.labels.track_add_options.clone();
    let log_section = state.labels.log_section.clone();
    let log_label = state.labels.log_label.clone();

    ui.painter().text(
        egui::pos2(left, base + 30.0),
        egui::Align2::LEFT_CENTER,
        &title,
        egui::FontId::proportional(theme::font::TITLE),
        palette.text,
    );

    // 表单：标签 + 间距 32（8+16+8，与生产 `row![label, space(16), pick_list]` 一致），
    // 控件起点随标签实际宽度变化——英文标签更长，固定列宽会压住下拉框。
    let label_x = left;
    form_row(
        ui,
        RowGeometry {
            label_x,
            center_y: base + 99.0,
        },
        &eraser_label,
        &state.eraser,
        &eraser_options,
        palette,
    );
    form_row(
        ui,
        RowGeometry {
            label_x,
            center_y: base + 171.0,
        },
        &track_add_label,
        &state.track_add,
        &track_add_options,
        palette,
    );

    ui.painter().text(
        egui::pos2(left, base + 236.0),
        egui::Align2::LEFT_CENTER,
        &log_section,
        egui::FontId::proportional(theme::font::SECTION),
        palette.text,
    );

    let log_label_width = ui
        .painter()
        .layout_no_wrap(
            log_label.clone(),
            egui::FontId::proportional(theme::font::LABEL),
            palette.text,
        )
        .size()
        .x;
    ui.painter().text(
        egui::pos2(label_x, base + 291.0),
        egui::Align2::LEFT_CENTER,
        &log_label,
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

/// 表单行几何（标签列起点 / 行中心）。
#[derive(Debug, Clone, Copy)]
struct RowGeometry {
    label_x: f32,
    center_y: f32,
}

fn form_row(
    ui: &mut egui::Ui,
    geometry: RowGeometry,
    label: &str,
    value: &str,
    options: &[String],
    palette: &SettingsPalette,
) {
    let label_font = egui::FontId::proportional(theme::font::LABEL);
    let label_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), label_font.clone(), palette.text)
        .size()
        .x;
    ui.painter().text(
        egui::pos2(geometry.label_x, geometry.center_y),
        egui::Align2::LEFT_CENTER,
        label,
        label_font,
        palette.text,
    );

    // 生产：`row![label, space(16), pick_list].spacing(8)` → 标签与控件间 8+16+8 = 32。
    let field_x = geometry.label_x + label_width + 32.0;
    let combo_rect = Rect::from_min_size(
        egui::pos2(field_x, geometry.center_y - 16.0),
        Vec2::new(200.0, 32.0),
    );
    let mut combo_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(combo_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    widgets::combo_box(&mut combo_ui, label, value, options, 200.0, palette);
}

fn confirm(ui: &mut egui::Ui, rect: Rect, palette: &SettingsPalette, label: &str) {
    let button = egui::Button::new(
        RichText::new(label)
            .size(theme::font::LABEL)
            .color(Color32::from_rgb(0x12, 0x1A, 0x2A)),
    )
    .fill(palette.accent)
    .corner_radius(CornerRadius::same(4));
    ui.put(rect, button);
}
