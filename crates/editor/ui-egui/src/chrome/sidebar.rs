//! 左侧栏：路由栏（48）+ 音轨面板（`panel_width`）。
//!
//! 路由栏条目与生产 `ROUTES` 一一对应：
//! - 组父按钮常驻，激活时点亮 `parent_color`；
//! - 子按钮仅在所属分组激活时出现，激活时点亮 `child_color`；
//! - 底部依次为横向 / 纵向卷帘按钮（`roll_bar_visible` 时）与混音台入口。

mod track_list;

use egui::{Color32, Rect, Vec2};

use super::{ChromeState, RollBar, SidebarGroup, SidebarModel, SidebarPanel};
use crate::icons::Icon;
use crate::theme;

/// 路由栏宽度（生产 `ROUTE_BAR_WIDTH`）。
pub(crate) const ROUTE_BAR_W: f32 = 48.0;
/// 路由按钮边长（生产 `bar_button` 固定 48×48）。
const ROUTE_BTN: f32 = 48.0;
/// 路由图标尺寸（生产 20×20）。
const ROUTE_ICON: f32 = 20.0;

/// 左侧栏：路由栏 + 音轨面板。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    show_model(ui, &state.sidebar.clone(), &mut state.icons);
}

/// 按只读模型渲染左侧栏。
pub fn show_model(ui: &mut egui::Ui, model: &SidebarModel, icons: &mut crate::icons::IconCache) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::PANEL_BG);
    let route = Rect::from_min_size(rect.min, Vec2::new(ROUTE_BAR_W, rect.height()));
    ui.painter().rect_filled(route, 0, theme::ROUTE_BAR_BG);

    route_bar(ui, rect, model, icons);

    if model.panel_visible {
        let panel = Rect::from_min_size(
            egui::pos2(rect.left() + ROUTE_BAR_W, rect.top()),
            Vec2::new((model.panel_width - 6.0).max(0.0), rect.height()),
        );
        match model.panel {
            SidebarPanel::Tracks => track_list::show_model(ui, panel, model, icons),
            SidebarPanel::AddTrackOnly => track_list::show_add_track(ui, panel, model, icons),
            SidebarPanel::Blank => {}
        }
    }
}

/// 路由栏：顶部条目 + 底部卷帘/混音台。
fn route_bar(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &SidebarModel,
    icons: &mut crate::icons::IconCache,
) {
    let left = rect.left();
    let mut y = rect.top();

    // PianoRoll 组父按钮：卷帘可见即点亮。
    let active = model
        .piano_roll_visible
        .then(|| SidebarGroup::PianoRoll.parent_color());
    route_button(ui, icons, Icon::Keys, left, y, active);
    y += ROUTE_BTN;

    if model.active_group == Some(SidebarGroup::PianoRoll) {
        let active = model
            .file_active
            .then(|| SidebarGroup::PianoRoll.child_color());
        route_button(ui, icons, Icon::FolderTree, left, y, active);
        y += ROUTE_BTN;

        let active = model
            .automation_panel_visible
            .then(|| SidebarGroup::PianoRoll.child_color());
        route_button(ui, icons, Icon::WaveForm, left, y, active);
        y += ROUTE_BTN;
    }

    let active = (model.active_group == Some(SidebarGroup::Project))
        .then(|| SidebarGroup::Project.parent_color());
    route_button(ui, icons, Icon::Arrangement, left, y, active);
    y += ROUTE_BTN;

    let active = model
        .waterfall
        .then(|| SidebarGroup::Waterfall.parent_color());
    route_button(ui, icons, Icon::PlayCircle, left, y, active);
    y += ROUTE_BTN;

    let active = (model.active_group == Some(SidebarGroup::Renderer))
        .then(|| SidebarGroup::Renderer.parent_color());
    route_button(ui, icons, Icon::Download, left, y, active);
    y += ROUTE_BTN;

    if model.active_group == Some(SidebarGroup::Renderer) {
        let active = model
            .video_export_visible
            .then(|| SidebarGroup::Renderer.child_color());
        route_button(ui, icons, Icon::VideoCamera, left, y, active);
        y += ROUTE_BTN;

        let active = model
            .audio_export_visible
            .then(|| SidebarGroup::Renderer.child_color());
        route_button(ui, icons, Icon::MusicNote, left, y, active);
    }

    // 底部：卷帘方向（可选）+ 混音台（常驻）。
    let mut bottom: Vec<(Icon, Option<Color32>)> = Vec::new();
    if model.roll_bar_visible {
        bottom.push((
            Icon::RollBarHorizontal,
            (model.roll_bar_active == Some(RollBar::Horizontal))
                .then(|| SidebarGroup::PianoRoll.child_color()),
        ));
        bottom.push((
            Icon::RollBarVertical,
            (model.roll_bar_active == Some(RollBar::Vertical))
                .then(|| SidebarGroup::PianoRoll.child_color()),
        ));
    }
    bottom.push((
        if model.mixer_open {
            Icon::MixerActive
        } else {
            Icon::Mixer
        },
        model
            .mixer_open
            .then(|| SidebarGroup::PianoRoll.child_color()),
    ));

    let mut by = rect.bottom() - ROUTE_BTN * bottom.len() as f32;
    for (icon, active) in bottom {
        route_button(ui, icons, icon, left, by, active);
        by += ROUTE_BTN;
    }
}

/// 单个路由按钮：左侧 2px 灯条 + 20×20 图标（图标起点 = 左 + 14）。
fn route_button(
    ui: &egui::Ui,
    icons: &mut crate::icons::IconCache,
    icon: Icon,
    left: f32,
    y: f32,
    active: Option<Color32>,
) {
    if let Some(color) = active {
        ui.painter().rect_filled(
            Rect::from_min_size(egui::pos2(left, y), Vec2::new(2.0, ROUTE_BTN)),
            0,
            color,
        );
    }
    let icon_rect = Rect::from_min_size(
        egui::pos2(left + 14.0, y + (ROUTE_BTN - ROUTE_ICON) / 2.0),
        Vec2::splat(ROUTE_ICON),
    );
    ui.painter().image(
        icons
            .texture(ui.ctx(), icon, ROUTE_ICON, icon.default_tint())
            .id(),
        icon_rect,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// 右侧栏图标列：48px 宽、按钮 48×48 + 2px 间距、灯条贴右边缘。
///
/// 只绘制图标列（`model.panel_visible` 的面板保持在 iced：其中含素材扫描、
/// 图片转换等复杂交互，本层暂不接管）。
pub fn show_right_model(
    ui: &mut egui::Ui,
    model: &super::RightSidebarModel,
    icons: &mut crate::icons::IconCache,
) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::ROUTE_BAR_BG);
    let left = rect.left();
    for (index, icon) in [
        Icon::ImageToMidi,
        Icon::MaterialLibrary,
        Icon::PianoWaterfall,
    ]
    .into_iter()
    .enumerate()
    {
        let y = rect.top() + index as f32 * (RIGHT_BTN + RIGHT_BTN_GAP);
        right_button(ui, icons, icon, left, y, model.active);
    }
}

/// 右侧栏按钮边长与间距（生产 `Column::spacing(2)`）。
const RIGHT_BTN: f32 = 48.0;
const RIGHT_BTN_GAP: f32 = 2.0;

/// 单个右侧栏按钮：图标在 46px 内容区居中（左侧灯条在右边缘 2px）。
fn right_button(
    ui: &egui::Ui,
    icons: &mut crate::icons::IconCache,
    icon: Icon,
    left: f32,
    y: f32,
    active: Option<super::RightSidebarPanelKind>,
) {
    let rect = Rect::from_min_size(egui::pos2(left, y), Vec2::splat(RIGHT_BTN));
    let lit = matches!(
        (active, icon),
        (
            Some(super::RightSidebarPanelKind::ImageToMidi),
            Icon::ImageToMidi
        ) | (
            Some(super::RightSidebarPanelKind::Materials),
            Icon::MaterialLibrary
        ) | (
            Some(super::RightSidebarPanelKind::PianoWaterfall),
            Icon::PianoWaterfall
        )
    );
    if lit {
        ui.painter().rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.right() - 2.0, rect.top()),
                Vec2::new(2.0, rect.height()),
            ),
            0,
            theme::ACCENT,
        );
    }
    // 内容区 = 48 - 2（灯条），图标 20 居中 → 起点 = 左 + 13。
    let icon_rect = Rect::from_min_size(
        egui::pos2(rect.left() + 13.0, rect.center().y - RIGHT_ICON / 2.0),
        Vec2::splat(RIGHT_ICON),
    );
    icons.paint(ui.painter(), icon, icon_rect);
}

/// 右侧栏图标尺寸（生产 20×20）。
const RIGHT_ICON: f32 = 20.0;
