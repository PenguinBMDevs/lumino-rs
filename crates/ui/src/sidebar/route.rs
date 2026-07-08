use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, row, space};
use lumino_core::i18n::Language;

use super::{Event, GroupId, ROUTES, Route, RouteConfig};
use crate::titlebar::mode_toggle::AppMode;

use crate::widget;
use crate::{Element, Theme, resources::icon, window};

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    active: Route,
    panel_visible: bool,
    automation_panel_visible: bool,
    piano_roll_visible: bool,
    current_mode: AppMode,
    active_group: Option<GroupId>,
    audio_export_visible: bool,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let items = ROUTES
        .into_iter()
        .map(|r| match r {
            // ── 组父按钮 ──
            RouteConfig::GroupParent { group, icon } => {
                let is_active = match group {
                    GroupId::PianoRoll => piano_roll_visible,
                    GroupId::Project => active_group == Some(GroupId::Project),
                    GroupId::Waterfall => current_mode == AppMode::Waterfall,
                    GroupId::Renderer => active_group == Some(GroupId::Renderer),
                };
                group_parent_item(group, icon, is_active, window, language)
            }
            // ── 路由项（子按钮） ──
            RouteConfig::Item { route, icon, group } => {
                // 子按钮仅当父组激活时可见
                if let Some(g) = group
                    && active_group != Some(g)
                {
                    // 不渲染，用空占位保持布局
                    return iced_widget::Space::new().width(48).height(0).into();
                }

                // 导出类按钮通过 sidebar 事件系统路由
                if matches!(route, Route::VideoExport | Route::AudioExport) {
                    let is_active = match route {
                        Route::AudioExport => audio_export_visible,
                        _ => false,
                    };
                    return export_item(
                        route,
                        icon,
                        is_active,
                        group.map(|g| g.child_color()),
                        window,
                        language,
                    );
                }

                let is_active = if active == Route::Arrangement {
                    route == Route::Arrangement
                } else if route == Route::Automation {
                    automation_panel_visible
                } else {
                    panel_visible && route == active
                };
                item_with_color(
                    route,
                    icon,
                    is_active,
                    group.map(|g| g.child_color()),
                    window,
                    language,
                )
            }
            RouteConfig::Space => space().height(Length::Fill).into(),
        })
        .collect::<Vec<_>>();

    container(column(items))
        .width(48)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weaker.color)
        })
        .into()
}

/// 渲染组父按钮（带硬编码分组颜色的指示条）
fn group_parent_item<'a>(
    group: GroupId,
    icon_enum: icon::Icon,
    active: bool,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let parent_color = group.parent_color();
    let split = container(space())
        .width(2)
        .height(Length::Fill)
        .style(move |_theme: &Theme| {
            let background = match active {
                true => parent_color,
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

    let event = match group {
        GroupId::PianoRoll => Event::group_toggled(GroupId::PianoRoll),
        GroupId::Project => Event::group_toggled(GroupId::Project),
        GroupId::Waterfall => Event::group_toggled(GroupId::Waterfall),
        GroupId::Renderer => Event::group_toggled(GroupId::Renderer),
    };

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
        .on_press(event);

    widget::with_tooltip(
        btn,
        group.tooltip(language),
        iced_widget::tooltip::Position::Right,
    )
    .into()
}

/// 渲染带可选颜色的路由项（子按钮用浅色指示条）
fn item_with_color<'a>(
    route: Route,
    icon_enum: icon::Icon,
    active: bool,
    indicator_color: Option<Color>,
    window: &window::Window,
    language: Language,
) -> Element<'a> {
    let indicator_color = indicator_color.unwrap_or(Color::TRANSPARENT);

    let split = container(space())
        .width(2)
        .height(Length::Fill)
        .style(move |_theme: &Theme| {
            // 使用传入的硬编码颜色
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

    let event = if route == Route::Automation {
        Event::automation_panel_toggled()
    } else {
        Event::panel_toggled(route)
    };

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
        .on_press(event);

    widget::with_tooltip(
        btn,
        route.tooltip(language),
        iced_widget::tooltip::Position::Right,
    )
    .into()
}

/// 渲染导出类子按钮（通过 sidebar 事件路由，由 root handler 打开对话框）
fn export_item<'a>(
    route: Route,
    icon_enum: icon::Icon,
    active: bool,
    indicator_color: Option<Color>,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let indicator_color = indicator_color.unwrap_or(Color::TRANSPARENT);

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

    // 通过 sidebar RouteUpdated 事件路由，由 root handler 拦截
    let event = Event::route_updated(route);

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
        .on_press(event);

    widget::with_tooltip(
        btn,
        route.tooltip(language),
        iced_widget::tooltip::Position::Right,
    )
    .into()
}
