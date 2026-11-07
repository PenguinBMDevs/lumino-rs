//! 设置对话框草稿：左侧菜单 + 右侧表单。
//!
//! 布局对齐生产 iced `ui-settings`：
//! - 内容区外边距 16，菜单 200 宽、卡片自适应，菜单与卡片间距 16；
//! - 菜单项行距 52，选中项只改文字颜色（无底色）；
//! - 卡片内边距 20，标题 18px，表单行距 72；
//! - 「确认」按钮在卡片之外、右下角 100×34。

mod action;
mod audio;
mod general;
mod pages;
mod ui;

pub use action::{SettingsAction, take_action};

use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Vec2};
use egui_extras::{Column, TableBuilder};

use crate::chrome::{SettingsLabels, SettingsPageModel, SettingsPalette};

const HITS_ID: &str = "lumino.settings.hits";

/// 本帧可点击控件的矩形（下划线页面渲染时填充）。
#[derive(Debug, Clone, Default)]
pub struct SettingsHits {
    /// 洋葱皮后端下拉框。
    pub onion_combo: Option<Rect>,
    /// 兼容性「检查」按钮。
    pub compat_check: Option<Rect>,
    /// 兼容性「复制诊断」按钮。
    pub compat_copy: Option<Rect>,
    /// 兼容性开关（标签 → 对钩框矩形）。
    pub compat_checks: Vec<(String, Rect)>,
    /// 左侧菜单行矩形（按菜单顺序）。
    pub menu_rows: Vec<Rect>,
    /// 音频页控件矩形（字段 → 矩形）。
    pub audio_rows: Vec<(crate::chrome::AudioField, Rect)>,
    /// 界面页控件矩形（字段 → 矩形）。
    pub ui_rows: Vec<(crate::chrome::UiField, Rect)>,
    /// 界面页主题卡片矩形（规范主题名 → 矩形）。
    pub theme_cards: Vec<(String, Rect)>,
}

/// 读取本帧命中区（未渲染对应页面时为空）。
pub fn hits(ctx: &egui::Context) -> SettingsHits {
    ctx.data(|d| d.get_temp(egui::Id::unique(HITS_ID)))
        .unwrap_or_default()
}

/// 页面渲染时写入命中区。
pub(super) fn set_hits(ctx: &egui::Context, hits: SettingsHits) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(HITS_ID), hits));
}
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
    /// 当前页内容（未迁移页面不会走到这里）。
    pub page: SettingsPageModel,
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
            page: SettingsPageModel::General,
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
    set_hits(ui.ctx(), SettingsHits::default());

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
    // 先取一份文案副本：行闭包需要可变借用 `state`（点击切换页签）。
    let labels = state.labels.menu.clone();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(1.0, 1.0)))
            .layout(Layout::top_down(Align::Min)),
    );

    // 10 项全部放得下时不引入 `ScrollArea`：既有像素基线逐点不变。
    let needed = layout::SETTINGS_MENU_ROW_H * MENU_ICONS.len() as f32;
    if needed <= child.max_rect().height() {
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
        return;
    }

    // 放不下时才是真滚动列表：叠加层已把滚轮同步给 egui，能滚到最后一项，
    // 因此不必再把整窗交回 iced（不削减任何菜单项）。
    // 滚动条按 iced `scrollable` 的 8px 实体条对齐（不悬浮）。
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 8.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = false;
    }
    egui::ScrollArea::vertical()
        .id_salt("settings-menu-scroll")
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, icon) in MENU_ICONS.iter().enumerate() {
                let label = labels.get(index).map_or("", String::as_str);
                menu_row(ui, state, index, label, *icon, palette);
            }
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
    let mut hits = hits(ui.ctx());
    hits.menu_rows.push(rect);
    set_hits(ui.ctx(), hits);
    if response.clicked() {
        state.selected = index;
        action::push_action(ui.ctx(), SettingsAction::MenuSelected(index));
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

/// 按当前页分发（页面模型逐帧克隆：字符串量级很小，避免与 `&mut state` 冲突）。
fn form(ui: &mut egui::Ui, rect: Rect, state: &mut SettingsState, palette: &SettingsPalette) {
    let page = state.page.clone();
    match page {
        SettingsPageModel::General => general::show(ui, rect, state, palette),
        SettingsPageModel::Shortcuts(model) => pages::shortcuts(ui, rect, &model, palette),
        SettingsPageModel::OnionSkin(model) => pages::onion_skin(ui, rect, &model, palette),
        SettingsPageModel::Compatibility(model) => pages::compatibility(ui, rect, &model, palette),
        SettingsPageModel::Audio(model) => {
            let height = audio::content_height(&model);
            scroll_page(ui, rect, "settings-audio-scroll", height, |ui, content| {
                audio::show(ui, content, &model, palette);
            });
        }
        SettingsPageModel::Ui(model) => {
            let height = ui::content_height(&model, rect.width());
            scroll_page(ui, rect, "settings-ui-scroll", height, |ui, content| {
                ui::show(ui, content, &model, palette);
            });
        }
    }
}

/// 页面内容滚动容器：只用于高度可能超出卡片的页面（常规页保持原几何）。
fn scroll_page(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &str,
    content_height: f32,
    add: impl FnOnce(&mut egui::Ui, Rect),
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(egui::Align::Min)),
    );
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 8.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = false;
    }
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let (_, content) = ui.allocate_space(Vec2::new(
                ui.available_width(),
                content_height.max(ui.available_height()),
            ));
            add(ui, content);
        });
}

fn confirm(ui: &mut egui::Ui, rect: Rect, palette: &SettingsPalette, label: &str) {
    let button = egui::Button::new(
        RichText::new(label)
            .size(theme::font::LABEL)
            .color(Color32::from_rgb(0x12, 0x1A, 0x2A)),
    )
    .fill(palette.accent)
    .corner_radius(CornerRadius::same(4));
    let response = ui.put(rect, button);
    if response.clicked() {
        action::push_action(ui.ctx(), SettingsAction::Confirm);
    }
}
