//! 左侧路由栏渲染 — 组父按钮、路由子按钮、导出按钮与卷帘面板底部按钮
//!
//! 全部按钮共用 `bar_button` 单一渲染实现：差异仅为灯条颜色、点击消息与提示文本。

use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, row, space};
use lumino_extras::i18n::Language;

use super::{Event, GroupId, ROUTES, RollBarButton, Route, RouteConfig};
use crate::titlebar::mode_toggle::AppMode;

use crate::widget;
use crate::{Element, Message, Theme, resources::icon, window};

/// 路由栏渲染参数
///
/// 参数聚合为结构体（替代 10 个位置参数），新增按钮时无需扩散函数签名。
pub struct RouteViewParams {
    /// 当前路由
    pub active: Route,
    /// 音轨列表面板是否可见
    pub panel_visible: bool,
    /// 自动化面板是否可见
    pub automation_panel_visible: bool,
    /// 钢琴卷帘是否可见
    pub piano_roll_visible: bool,
    /// 当前应用模式（编辑器/瀑布流）
    pub current_mode: AppMode,
    /// 当前激活分组
    pub active_group: Option<GroupId>,
    /// 音频渲染面板是否可见
    pub audio_export_visible: bool,
    /// 视频渲染面板是否可见
    pub video_export_visible: bool,
    /// 卷帘面板底部按钮激活项（`None` = 两个按钮均未点亮）
    pub roll_bar_active: Option<RollBarButton>,
    /// 是否渲染卷帘面板底部按钮（仅在处于卷帘面板时为 true）
    pub roll_bar_visible: bool,
    /// 混音台浮动面板是否打开（用于点亮入口按钮）
    pub mixer_panel_open: bool,
}

pub fn view<'a>(
    params: RouteViewParams,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let mut items = ROUTES
        .into_iter()
        .map(|r| match r {
            // ── 组父按钮 ──
            RouteConfig::GroupParent { group, icon } => {
                let is_active = match group {
                    GroupId::PianoRoll => params.piano_roll_visible,
                    GroupId::Project => params.active_group == Some(GroupId::Project),
                    GroupId::Waterfall => params.current_mode == AppMode::Waterfall,
                    GroupId::Renderer => params.active_group == Some(GroupId::Renderer),
                };
                bar_button(
                    icon,
                    is_active,
                    group.parent_color(),
                    Event::group_toggled(group),
                    group.tooltip(language),
                    window,
                )
            }
            // ── 路由项（子按钮） ──
            RouteConfig::Item { route, icon, group } => {
                // 子按钮仅当父组激活时可见
                if let Some(g) = group
                    && params.active_group != Some(g)
                {
                    // 不渲染，用空占位保持布局
                    return iced_widget::Space::new().width(48).height(0).into();
                }

                let indicator = group.map_or(Color::TRANSPARENT, |g| g.child_color());

                // 导出类按钮通过 sidebar 事件系统路由，由 root handler 拦截
                if matches!(route, Route::VideoExport | Route::AudioExport) {
                    let is_active = match route {
                        Route::AudioExport => params.audio_export_visible,
                        Route::VideoExport => params.video_export_visible,
                        _ => false,
                    };
                    return bar_button(
                        icon,
                        is_active,
                        indicator,
                        Event::route_updated(route),
                        route.tooltip(language),
                        window,
                    );
                }

                let is_active = if params.active == Route::Arrangement {
                    route == Route::Arrangement
                } else if route == Route::Automation {
                    params.automation_panel_visible
                } else {
                    params.panel_visible && route == params.active
                };
                let event = if route == Route::Automation {
                    Event::automation_panel_toggled()
                } else {
                    Event::panel_toggled(route)
                };
                bar_button(
                    icon,
                    is_active,
                    indicator,
                    event,
                    route.tooltip(language),
                    window,
                )
            }
            RouteConfig::Space => space().height(Length::Fill).into(),
        })
        .collect::<Vec<_>>();

    // ── 卷帘面板底部按钮（仅处于卷帘面板时显示） ──
    // ROUTES 末项为 `Space`（Length::Fill），因此以下按钮被推到左侧栏底部；
    // 追加顺序决定纵向排布：横向三条杠在上，纵向三条杠在最下方。
    if params.roll_bar_visible {
        items.push(roll_bar_button(
            RollBarButton::Horizontal,
            params.roll_bar_active,
            window,
            language,
        ));
        items.push(roll_bar_button(
            RollBarButton::Vertical,
            params.roll_bar_active,
            window,
            language,
        ));
    }

    // 混音台入口按钮（点亮表示面板打开）；常驻左侧栏底部，点击切换浮动面板。
    items.push(bar_button(
        if params.mixer_panel_open {
            icon::MixerActive
        } else {
            icon::Mixer
        },
        params.mixer_panel_open,
        GroupId::PianoRoll.child_color(),
        Event::mixer_panel_toggled(),
        "混音台",
        window,
    ));

    container(column(items))
        .width(48)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weaker.color)
        })
        .into()
}

/// 渲染卷帘面板底部按钮（横向/纵向三条杠）
///
/// 亮灯能力与普通子按钮一致：复用钢琴卷帘组子按钮灯条颜色，
/// 激活项由 `active` 单值决定，天然互斥。
fn roll_bar_button<'a>(
    kind: RollBarButton,
    active: Option<RollBarButton>,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let icon_enum = match kind {
        RollBarButton::Horizontal => icon::RollBarHorizontal,
        RollBarButton::Vertical => icon::RollBarVertical,
    };
    bar_button(
        icon_enum,
        active == Some(kind),
        GroupId::PianoRoll.child_color(),
        Event::roll_bar_toggled(kind),
        kind.tooltip(language),
        window,
    )
}

/// 路由栏按钮统一渲染：左侧 2px 灯条 + 20x20 图标 + 悬浮提示
///
/// `active` 为 true 时灯条以 `indicator_color` 点亮，否则透明（熄灭）。
fn bar_button<'a>(
    icon_enum: icon::Icon,
    active: bool,
    indicator_color: Color,
    message: Message,
    tooltip: &'static str,
    window: &'a window::Window,
) -> Element<'a> {
    let split = container(space())
        .width(2)
        .height(Length::Fill)
        .style(move |_theme: &Theme| {
            let background = match active {
                true => indicator_color,
                false => Color::TRANSPARENT,
            };
            container::Style::default().background(background)
        });

    let icon_img = icon::view_with_size_and_theme(icon_enum, 20, 20, Some(&window.theme));

    let inner = row![split, icon_img,]
        .spacing(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center);

    let btn = button(inner)
        .width(48)
        .height(48)
        .padding(0)
        .style(move |theme: &Theme, status| {
            use button::Status::*;
            let p = theme.extended_palette();
            let text_color = match status {
                Hovered | Pressed => p.background.base.color,
                _ => p.background.weakest.color,
            };
            button::Style {
                text_color,
                ..Default::default()
            }
            .with_background(Color::TRANSPARENT)
        })
        .on_press(message);

    widget::with_tooltip(btn, tooltip, iced_widget::tooltip::Position::Right).into()
}
