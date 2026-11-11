//! 左侧栏：路由栏（48）+ 音轨面板（`panel_width`）。
//!
//! 路由栏条目与生产 `ROUTES` 一一对应：
//! - 组父按钮常驻，激活时点亮 `parent_color`；
//! - 子按钮仅在所属分组激活时出现，激活时点亮 `child_color`；
//! - 底部依次为横向 / 纵向卷帘按钮（`roll_bar_visible` 时）与混音台入口。

mod menu;
mod panel_action;
mod picker;
mod track_list;

use egui::{Color32, Rect, Sense, Vec2};

use super::{ChromeState, RollBar, SidebarGroup, SidebarModel, SidebarPanel, SidebarRouteIcon};
use crate::icons::Icon;
use crate::theme;
pub use panel_action::{
    SidebarMenuAction, SidebarPanelAction, take_menu_action, take_panel_action,
};

/// 路由栏宽度（生产 `ROUTE_BAR_WIDTH`）。
pub(crate) const ROUTE_BAR_W: f32 = 48.0;

/// 路由栏条目（与生产 `ROUTES` 语义一一对应，不含 iced 类型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SidebarRoute {
    /// 钢琴卷帘组父按钮。
    PianoRollGroup,
    /// 音轨列表子按钮。
    FilePanel,
    /// 自动化面板子按钮。
    AutomationPanel,
    /// 工程走带组父按钮。
    ProjectGroup,
    /// 播放器（瀑布流）组父按钮。
    WaterfallGroup,
    /// 渲染组父按钮。
    RendererGroup,
    /// 视频导出子按钮。
    VideoExport,
    /// 音频导出子按钮。
    AudioExport,
    /// 底部：横向卷帘方向。
    RollBarHorizontal,
    /// 底部：纵向卷帘方向。
    RollBarVertical,
    /// 底部：混音台开关。
    MixerToggle,
}

/// 路由栏动作通道。
const ROUTE_ACTION_ID: &str = "lumino.sidebar.route-action";

/// 记录本帧路由栏点击（一帧一条）。
pub fn push_route_action(ctx: &egui::Context, route: SidebarRoute) {
    if ctx
        .data(|d| d.get_temp::<Option<SidebarRoute>>(egui::Id::unique(ROUTE_ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ROUTE_ACTION_ID), Some(route)));
}

/// 取走本帧路由栏点击。
pub fn take_route_action(ctx: &egui::Context) -> Option<SidebarRoute> {
    ctx.data_mut(|d| d.remove_temp::<Option<SidebarRoute>>(egui::Id::unique(ROUTE_ACTION_ID)))
        .flatten()
}

/// 调试开关：`LUMINO_EGUI_DEBUG_RECTS=1` 时把每个路由按钮的矩形打进日志，
/// 用于和 iced 的命中盒逐项对齐（真机验收不必再靠截图猜坐标）。
fn debug_rects() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0"))
}
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
    ui.painter().rect_filled(rect, 0, theme::panel_bg());
    let route = Rect::from_min_size(rect.min, Vec2::new(ROUTE_BAR_W, rect.height()));
    ui.painter().rect_filled(route, 0, theme::route_bar_bg());

    // 路由栏由 egui **独占**：绘制 + 点击 + 动作通道（含底部卷帘方向与混音台）。
    // 因此必须计入占用区，否则同一次点击会同时被 egui 和 iced 各处理一次
    // （表现为「打开又立刻关闭」，或命中到 iced 的旧偏移坐标）。
    super::menu::extend_blocked(ui.ctx(), [route]);

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
        // 右侧 6px 缩放手柄：iced 版本会被底层画布纹理盖住，这里由 egui 绘制并接管。
        let handle = Rect::from_min_size(
            egui::pos2(panel.right(), panel.top()),
            Vec2::new(6.0, panel.height()),
        );
        ui.painter().rect_filled(
            handle,
            0,
            if model.resizing {
                theme::accent_strong()
            } else {
                theme::panel_bg()
            },
        );
        let response = ui.interact(
            handle,
            egui::Id::unique("lumino.sidebar.resize"),
            egui::Sense::click_and_drag(),
        );
        let response = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        let dragging = egui::Id::unique("lumino.sidebar.resizing");
        let started = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(dragging))
            .unwrap_or(false);
        let down = response.is_pointer_button_down_on();
        if down && !started {
            ui.ctx().data_mut(|d| d.insert_temp(dragging, true));
            panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::ResizeStarted);
        } else if !down && started {
            ui.ctx().data_mut(|d| d.insert_temp(dragging, false));
            panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::ResizeEnded);
        }
        crate::chrome::menu::extend_blocked(ui.ctx(), [handle]);
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

    // 生产已给出条目表时按表绘制：顺序 / 可见性 / 点亮与 iced 同源，
    // 避免两边各自推导导致「看到的图标」与「点得中的按钮」错位。
    if !model.route_items.is_empty() {
        for item in &model.route_items {
            let icon = match item.icon {
                SidebarRouteIcon::PianoRoll => Icon::Keys,
                SidebarRouteIcon::File => Icon::FolderTree,
                SidebarRouteIcon::Automation => Icon::WaveForm,
                SidebarRouteIcon::Project => Icon::Arrangement,
                SidebarRouteIcon::Waterfall => Icon::PlayCircle,
                SidebarRouteIcon::Renderer => Icon::Download,
                SidebarRouteIcon::VideoExport => Icon::VideoCamera,
                SidebarRouteIcon::AudioExport => Icon::MusicNote,
            };
            let active = item.active.then(|| route_indicator(item.icon));
            let route = match item.icon {
                SidebarRouteIcon::PianoRoll => SidebarRoute::PianoRollGroup,
                SidebarRouteIcon::File => SidebarRoute::FilePanel,
                SidebarRouteIcon::Automation => SidebarRoute::AutomationPanel,
                SidebarRouteIcon::Project => SidebarRoute::ProjectGroup,
                SidebarRouteIcon::Waterfall => SidebarRoute::WaterfallGroup,
                SidebarRouteIcon::Renderer => SidebarRoute::RendererGroup,
                SidebarRouteIcon::VideoExport => SidebarRoute::VideoExport,
                SidebarRouteIcon::AudioExport => SidebarRoute::AudioExport,
            };
            route_button(ui, icons, icon, left, y, active, Some(route));
            y += ROUTE_BTN;
        }
        route_bar_bottom(ui, rect, model, icons);
        return;
    }

    // PianoRoll 组父按钮：卷帘可见即点亮。
    let active = model
        .piano_roll_visible
        .then(|| SidebarGroup::PianoRoll.parent_color());
    route_button(
        ui,
        icons,
        Icon::Keys,
        left,
        y,
        active,
        Some(SidebarRoute::PianoRollGroup),
    );
    y += ROUTE_BTN;

    if model.active_group == Some(SidebarGroup::PianoRoll) {
        let active = model
            .file_active
            .then(|| SidebarGroup::PianoRoll.child_color());
        route_button(
            ui,
            icons,
            Icon::FolderTree,
            left,
            y,
            active,
            Some(SidebarRoute::FilePanel),
        );
        y += ROUTE_BTN;

        let active = model
            .automation_panel_visible
            .then(|| SidebarGroup::PianoRoll.child_color());
        route_button(
            ui,
            icons,
            Icon::WaveForm,
            left,
            y,
            active,
            Some(SidebarRoute::AutomationPanel),
        );
        y += ROUTE_BTN;
    }

    let active = (model.active_group == Some(SidebarGroup::Project))
        .then(|| SidebarGroup::Project.parent_color());
    route_button(
        ui,
        icons,
        Icon::Arrangement,
        left,
        y,
        active,
        Some(SidebarRoute::ProjectGroup),
    );
    y += ROUTE_BTN;

    let active = model
        .waterfall
        .then(|| SidebarGroup::Waterfall.parent_color());
    route_button(
        ui,
        icons,
        Icon::PlayCircle,
        left,
        y,
        active,
        Some(SidebarRoute::WaterfallGroup),
    );
    y += ROUTE_BTN;

    let active = (model.active_group == Some(SidebarGroup::Renderer))
        .then(|| SidebarGroup::Renderer.parent_color());
    route_button(
        ui,
        icons,
        Icon::Download,
        left,
        y,
        active,
        Some(SidebarRoute::RendererGroup),
    );
    y += ROUTE_BTN;

    // 渲染组子按钮（视频 / 音频导出）：仅组激活时出现。
    if model.active_group == Some(SidebarGroup::Renderer) {
        let active = model
            .video_export_visible
            .then(|| SidebarGroup::Renderer.child_color());
        route_button(
            ui,
            icons,
            Icon::VideoCamera,
            left,
            y,
            active,
            Some(SidebarRoute::VideoExport),
        );
        y += ROUTE_BTN;

        let active = model
            .audio_export_visible
            .then(|| SidebarGroup::Renderer.child_color());
        route_button(
            ui,
            icons,
            Icon::MusicNote,
            left,
            y,
            active,
            Some(SidebarRoute::AudioExport),
        );
    }

    // 底部：卷帘方向（可选）+ 混音台（常驻）。
    route_bar_bottom(ui, rect, model, icons);
}

/// 灯条颜色（按条目所属分组）。
fn route_indicator(icon: SidebarRouteIcon) -> Color32 {
    match icon {
        SidebarRouteIcon::PianoRoll | SidebarRouteIcon::File | SidebarRouteIcon::Automation => {
            SidebarGroup::PianoRoll.child_color()
        }
        SidebarRouteIcon::Project => SidebarGroup::Project.parent_color(),
        SidebarRouteIcon::Waterfall => SidebarGroup::Waterfall.parent_color(),
        SidebarRouteIcon::Renderer
        | SidebarRouteIcon::VideoExport
        | SidebarRouteIcon::AudioExport => SidebarGroup::Renderer.child_color(),
    }
}

/// 路由栏底部：卷帘方向按钮 + 混音台入口。
fn route_bar_bottom(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &SidebarModel,
    icons: &mut crate::icons::IconCache,
) {
    let left = rect.left();
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

    // Ui 的高度可能是无界的（父级 `max_rect` 未定高），直接用 `rect.bottom()`
    // 会把底部按钮推到屏幕外（实测有一趟 pass 底缘 = 7980）。
    // 因此优先用宿主给出的真实窗口高度锚定，其次才回退到 Ui/视口。
    let viewport_bottom = super::window_size(ui.ctx())
        .map(|size| size.y - super::layout::STATUSBAR_H)
        .unwrap_or_else(|| ui.ctx().viewport_rect().bottom() - super::layout::STATUSBAR_H);
    let bottom_edge = rect.bottom().min(viewport_bottom);
    if debug_rects() {
        tracing::warn!(?rect, viewport_bottom, bottom_edge, "DEBUG route bar rect");
    }
    let mut by = bottom_bar_origin(bottom_edge, bottom.len());
    // 底部按钮同样经动作通道下发：`ui.interact` 会吃掉点击，
    // 传 `None` 就成了「吃了事件还不响应」的死按钮（真机表现：混音台点不开）。
    let mut routes = Vec::new();
    if model.roll_bar_visible {
        routes.push(SidebarRoute::RollBarHorizontal);
        routes.push(SidebarRoute::RollBarVertical);
    }
    routes.push(SidebarRoute::MixerToggle);

    for ((icon, active), route) in bottom.into_iter().zip(routes) {
        route_button(ui, icons, icon, left, by, active, Some(route));
        by += ROUTE_BTN;
    }
}

/// 底部按钮组的第一项 y：整组贴 `bottom_edge` 向上排。
///
/// 抽成纯函数的原因：UI 侧 `rect.bottom()` 在部分 frame 会无界（实测 7980），
/// 这里只接受**已钳制**的底缘，便于单测锁定「混音台永远贴底」这一不变量。
fn bottom_bar_origin(bottom_edge: f32, count: usize) -> f32 {
    bottom_edge - ROUTE_BTN * count as f32
}

/// 单个路由按钮：左侧 2px 灯条 + 20×20 图标（图标起点 = 左 + 14）。
fn route_button(
    ui: &egui::Ui,
    icons: &mut crate::icons::IconCache,
    icon: Icon,
    left: f32,
    y: f32,
    active: Option<Color32>,
    route: Option<SidebarRoute>,
) {
    // 路由栏由 egui 不透明绘制，必须同时接管点击：否则 iced 的同位置按钮被盖住，
    // 或者两边各自命中导致「打开又立刻关闭」。
    let hit = Rect::from_min_size(egui::pos2(left, y), Vec2::new(ROUTE_BAR_W, ROUTE_BTN));
    let response = ui.interact(
        hit,
        egui::Id::unique(("lumino.route", left as i32, y as i32)),
        Sense::click(),
    );
    if debug_rects() {
        tracing::warn!(
            ?route,
            x = left,
            y,
            hovered = response.hovered(),
            clicked = response.clicked(),
            pointer = ?ui.ctx().pointer_latest_pos(),
            "DEBUG route button rect"
        );
    }
    if let Some(route) = route {
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                true,
                active.is_some(),
                format!("{route:?}"),
            )
        });
        if response.clicked() {
            push_route_action(ui.ctx(), route);
        }
    }
    if response.hovered() {
        ui.painter()
            .rect_filled(hit, 0, Color32::from_white_alpha(10));
    }
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
/// 图标列由 egui **独占**：绘制 + 点击 + 动作通道（`RailClicked`）。
/// 因此整列计入占用区，否则同一次点击会同时被 egui 与 iced 的旧按钮各处理一次。
pub fn show_right_model(
    ui: &mut egui::Ui,
    model: &super::RightSidebarModel,
    icons: &mut crate::icons::IconCache,
) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::route_bar_bg());
    let left = rect.left();
    // 生产侧总会给出三项（含 i18n 文案）；测试构造的模型可能为空，回退到内置顺序。
    let fallback = [
        (
            super::RightSidebarPanelKind::ImageToMidi,
            Icon::ImageToMidi,
            "",
        ),
        (
            super::RightSidebarPanelKind::Materials,
            Icon::MaterialLibrary,
            "",
        ),
        (
            super::RightSidebarPanelKind::PianoWaterfall,
            Icon::PianoWaterfall,
            "",
        ),
    ];
    let items: Vec<(super::RightSidebarPanelKind, Icon, &str)> = if model.rail.is_empty() {
        fallback
            .iter()
            .map(|(kind, icon, label)| (*kind, *icon, *label))
            .collect()
    } else {
        model
            .rail
            .iter()
            .map(|item| {
                let icon = match item.kind {
                    super::RightSidebarPanelKind::ImageToMidi => Icon::ImageToMidi,
                    super::RightSidebarPanelKind::Materials => Icon::MaterialLibrary,
                    super::RightSidebarPanelKind::PianoWaterfall => Icon::PianoWaterfall,
                };
                (item.kind, icon, item.label.as_str())
            })
            .collect()
    };

    for (index, (kind, icon, label)) in items.into_iter().enumerate() {
        let y = rect.top() + index as f32 * (RIGHT_BTN + RIGHT_BTN_GAP);
        right_button(ui, icons, icon, left, y, model.active, Some((kind, label)));
    }
    // 整列占用：点击只给 egui（iced 的同位置按钮不再收到事件）。
    super::menu::extend_blocked(ui.ctx(), [rect]);
}

/// 右侧栏按钮边长与间距（生产 `Column::spacing(2)`）。
const RIGHT_BTN: f32 = 48.0;
const RIGHT_BTN_GAP: f32 = 2.0;

/// 单个右侧栏按钮：图标在 46px 内容区居中（灯条在右边缘 2px）。
fn right_button(
    ui: &egui::Ui,
    icons: &mut crate::icons::IconCache,
    icon: Icon,
    left: f32,
    y: f32,
    active: Option<super::RightSidebarPanelKind>,
    route: Option<(super::RightSidebarPanelKind, &str)>,
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
    if let Some((kind, label)) = route {
        let response = ui.interact(
            rect,
            egui::Id::unique(("lumino.right-rail", y as i32)),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, true, lit, format!("{kind:?}"))
        });
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, 0, Color32::from_white_alpha(10));
        }
        if response.clicked() {
            super::right_panel::push_action(
                ui.ctx(),
                super::right_panel::RightSidebarAction::RailClicked(kind),
            );
        }
        if !label.is_empty() {
            response.on_hover_text(label);
        }
    }
    // 灯条画在悬停底色之后，避免被覆盖。
    if lit {
        ui.painter().rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.right() - 2.0, rect.top()),
                Vec2::new(2.0, rect.height()),
            ),
            0,
            theme::accent(),
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

#[cfg(test)]
mod bottom_bar_tests {
    use super::*;

    #[test]
    fn bottom_bar_stacks_up_from_window_bottom() {
        // 窗口 1232、状态栏 20 → 底缘 1212；三个按钮（横/纵/混音台）。
        let origin = bottom_bar_origin(1212.0, 3);
        assert!((origin - 1068.0).abs() < 0.01);
        // 混音台是最后一项 → 恰好贴底（1164..1212）。
        let mixer_y = origin + 2.0 * ROUTE_BTN;
        assert!((mixer_y - 1164.0).abs() < 0.01);
        assert!(
            (mixer_y + ROUTE_BTN - 1212.0).abs() < 0.01,
            "混音台必须贴到状态栏上沿"
        );
    }

    #[test]
    fn bottom_bar_handles_single_button() {
        // 只有混音台时：直接贴底。
        assert!((bottom_bar_origin(1212.0, 1) - 1164.0).abs() < 0.01);
    }
}
