//! 路由栏：条目表 / 内置顺序两条路径 + 底部卷帘与混音台。
//!
//! 条目按模型给出的顺序绘制；`route_items` 为空时回退到内置分组顺序。
//! 两条路径共用 `route_button` 与 `route_bar_bottom`，命中与动作通道只有一份。

use egui::{Color32, Rect, Vec2};

use crate::chrome::{RollBar, SidebarGroup, SidebarModel, SidebarRouteIcon};
use crate::icons::Icon;
use crate::widgets::HoverTip;

use super::{ROUTE_BAR_W, SidebarRoute, push_route_action};

/// 调试开关：`LUMINO_EGUI_DEBUG_RECTS=1` 时把每个路由按钮的矩形打进日志，
/// 用于和 上一代 的命中盒逐项对齐（真机验收不必再靠截图猜坐标）。
fn debug_rects() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0"))
}
/// 路由按钮边长（生产 `bar_button` 固定 48×48）。
const ROUTE_BTN: f32 = 48.0;
/// 路由图标尺寸（生产 20×20）。
const ROUTE_ICON: f32 = 20.0;

/// 路由栏：顶部条目 + 底部卷帘/混音台。
pub(super) fn route_bar(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &SidebarModel,
    icons: &mut crate::icons::IconCache,
) {
    let left = rect.left();
    let mut y = rect.top();

    // 生产已给出条目表时按表绘制：顺序 / 可见性 / 点亮与 上一代 同源，
    // 避免两边各自推导导致「看到的图标」与「点得中的按钮」错位。
    if !model.route_items.is_empty() {
        route_items(ui, model, icons, left, &mut y);
    } else {
        builtin_items(ui, model, icons, left, &mut y);
    }

    // 底部：卷帘方向（可选）+ 混音台（常驻）。
    route_bar_bottom(ui, rect, model, icons);
}

/// 按模型条目表绘制（生产路径）。
fn route_items(
    ui: &mut egui::Ui,
    model: &SidebarModel,
    icons: &mut crate::icons::IconCache,
    left: f32,
    y: &mut f32,
) {
    for item in &model.route_items {
        let (icon, route) = route_target(item.icon);
        let active = item.active.then(|| route_indicator(item.icon));
        route_button(
            ui,
            icons,
            icon,
            egui::pos2(left, *y),
            active,
            Some(route),
            Some(item.tip),
        );
        *y += ROUTE_BTN;
    }
}

/// 内置分组顺序（测试 / 模型缺省路径）：父按钮常驻，子按钮仅在组激活时出现。
fn builtin_items(
    ui: &mut egui::Ui,
    model: &SidebarModel,
    icons: &mut crate::icons::IconCache,
    left: f32,
    y: &mut f32,
) {
    piano_roll_group(ui, model, icons, left, y);
    parent_button(
        ui,
        icons,
        egui::pos2(left, *y),
        model.active_group == Some(SidebarGroup::Project),
        GroupButton {
            icon: Icon::Arrangement,
            group: SidebarGroup::Project,
            route: SidebarRoute::ProjectGroup,
        },
    );
    *y += ROUTE_BTN;
    parent_button(
        ui,
        icons,
        egui::pos2(left, *y),
        model.waterfall,
        GroupButton {
            icon: Icon::PlayCircle,
            group: SidebarGroup::Waterfall,
            route: SidebarRoute::WaterfallGroup,
        },
    );
    *y += ROUTE_BTN;
    renderer_group(ui, model, icons, left, y);
}

/// PianoRoll 组：父按钮常驻，File / Automation 仅组激活时出现。
fn piano_roll_group(
    ui: &mut egui::Ui,
    model: &SidebarModel,
    icons: &mut crate::icons::IconCache,
    left: f32,
    y: &mut f32,
) {
    parent_button(
        ui,
        icons,
        egui::pos2(left, *y),
        model.piano_roll_visible,
        GroupButton {
            icon: Icon::Keys,
            group: SidebarGroup::PianoRoll,
            route: SidebarRoute::PianoRollGroup,
        },
    );
    *y += ROUTE_BTN;
    if model.active_group == Some(SidebarGroup::PianoRoll) {
        child_button(
            ui,
            icons,
            egui::pos2(left, *y),
            model.file_active,
            GroupButton {
                icon: Icon::FolderTree,
                group: SidebarGroup::PianoRoll,
                route: SidebarRoute::FilePanel,
            },
        );
        *y += ROUTE_BTN;
        child_button(
            ui,
            icons,
            egui::pos2(left, *y),
            model.automation_panel_visible,
            GroupButton {
                icon: Icon::WaveForm,
                group: SidebarGroup::PianoRoll,
                route: SidebarRoute::AutomationPanel,
            },
        );
        *y += ROUTE_BTN;
    }
}

/// Renderer 组：父按钮常驻，视频 / 音频导出仅组激活时出现。
fn renderer_group(
    ui: &mut egui::Ui,
    model: &SidebarModel,
    icons: &mut crate::icons::IconCache,
    left: f32,
    y: &mut f32,
) {
    parent_button(
        ui,
        icons,
        egui::pos2(left, *y),
        model.active_group == Some(SidebarGroup::Renderer),
        GroupButton {
            icon: Icon::Download,
            group: SidebarGroup::Renderer,
            route: SidebarRoute::RendererGroup,
        },
    );
    *y += ROUTE_BTN;
    if model.active_group == Some(SidebarGroup::Renderer) {
        child_button(
            ui,
            icons,
            egui::pos2(left, *y),
            model.video_export_visible,
            GroupButton {
                icon: Icon::VideoCamera,
                group: SidebarGroup::Renderer,
                route: SidebarRoute::VideoExport,
            },
        );
        *y += ROUTE_BTN;
        child_button(
            ui,
            icons,
            egui::pos2(left, *y),
            model.audio_export_visible,
            GroupButton {
                icon: Icon::MusicNote,
                group: SidebarGroup::Renderer,
                route: SidebarRoute::AudioExport,
            },
        );
        *y += ROUTE_BTN;
    }
}

/// 组按钮的图标与路由：与点亮级别（父 / 子）分开传，避免参数表过长。
struct GroupButton {
    icon: Icon,
    group: SidebarGroup,
    route: SidebarRoute,
}

/// 组父按钮：点亮用 `parent_color`。
fn parent_button(
    ui: &mut egui::Ui,
    icons: &mut crate::icons::IconCache,
    pos: egui::Pos2,
    lit: bool,
    spec: GroupButton,
) {
    let active = lit.then(|| spec.group.parent_color());
    route_button(ui, icons, spec.icon, pos, active, Some(spec.route), None);
}

/// 组子按钮：点亮用 `child_color`。
fn child_button(
    ui: &mut egui::Ui,
    icons: &mut crate::icons::IconCache,
    pos: egui::Pos2,
    lit: bool,
    spec: GroupButton,
) {
    let active = lit.then(|| spec.group.child_color());
    route_button(ui, icons, spec.icon, pos, active, Some(spec.route), None);
}

/// 条目图标 →（绘制图标，路由动作）。
fn route_target(icon: SidebarRouteIcon) -> (Icon, SidebarRoute) {
    match icon {
        SidebarRouteIcon::PianoRoll => (Icon::Keys, SidebarRoute::PianoRollGroup),
        SidebarRouteIcon::File => (Icon::FolderTree, SidebarRoute::FilePanel),
        SidebarRouteIcon::Automation => (Icon::WaveForm, SidebarRoute::AutomationPanel),
        SidebarRouteIcon::Project => (Icon::Arrangement, SidebarRoute::ProjectGroup),
        SidebarRouteIcon::Waterfall => (Icon::PlayCircle, SidebarRoute::WaterfallGroup),
        SidebarRouteIcon::Renderer => (Icon::Download, SidebarRoute::RendererGroup),
        SidebarRouteIcon::VideoExport => (Icon::VideoCamera, SidebarRoute::VideoExport),
        SidebarRouteIcon::AudioExport => (Icon::MusicNote, SidebarRoute::AudioExport),
    }
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
    let mut bottom: Vec<(Icon, Option<Color32>, HoverTip)> = Vec::new();
    if model.roll_bar_visible {
        bottom.push((
            Icon::RollBarHorizontal,
            (model.roll_bar_active == Some(RollBar::Horizontal))
                .then(|| SidebarGroup::PianoRoll.child_color()),
            model.roll_bar_horizontal_tip,
        ));
        bottom.push((
            Icon::RollBarVertical,
            (model.roll_bar_active == Some(RollBar::Vertical))
                .then(|| SidebarGroup::PianoRoll.child_color()),
            model.roll_bar_vertical_tip,
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
        model.mixer_tip,
    ));

    // Ui 的高度可能是无界的（父级 `max_rect` 未定高），直接用 `rect.bottom()`
    // 会把底部按钮推到屏幕外（实测有一趟 pass 底缘 = 7980）。
    // 因此优先用宿主给出的真实窗口高度锚定，其次才回退到 Ui/视口。
    let viewport_bottom = crate::chrome::window_size(ui.ctx())
        .map(|size| size.y - crate::layout::STATUSBAR_H)
        .unwrap_or_else(|| ui.ctx().viewport_rect().bottom() - crate::layout::STATUSBAR_H);
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

    for ((icon, active, tip), route) in bottom.into_iter().zip(routes) {
        route_button(
            ui,
            icons,
            icon,
            egui::pos2(left, by),
            active,
            Some(route),
            Some(tip),
        );
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
    pos: egui::Pos2,
    active: Option<Color32>,
    route: Option<SidebarRoute>,
    tip: Option<HoverTip>,
) {
    let (left, y) = (pos.x, pos.y);
    // 路由栏由 egui 不透明绘制，必须同时接管点击：否则 上一代 的同位置按钮被盖住，
    // 或者两边各自命中导致「打开又立刻关闭」。
    let hit = Rect::from_min_size(egui::pos2(left, y), Vec2::new(ROUTE_BAR_W, ROUTE_BTN));
    let response = ui.interact(
        hit,
        egui::Id::unique(("lumino.route", left as i32, y as i32)),
        egui::Sense::click(),
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
    if let Some(tip) = tip {
        let _ = crate::widgets::hover_tip(response, tip);
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
