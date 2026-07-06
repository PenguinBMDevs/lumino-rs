use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, row, space};
use lumino_core::i18n::Language;

use super::{Event, ROUTES, Route, RouteConfig};
use crate::message::{AudioExportAction, Message};
use crate::titlebar::mode_toggle::AppMode;

use crate::widget;
use crate::{Element, Theme, resources::icon, window};

pub fn view<'a>(
    active: Route,
    panel_visible: bool,
    automation_panel_visible: bool,
    piano_roll_visible: bool,
    current_mode: AppMode,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let items = ROUTES
        .into_iter()
        .map(|r| match r {
            RouteConfig::Item { route, icon } => {
                // 工程走带模式下：只有 Arrangement 按钮亮，其他按钮全部熄灭
                // 因为工程走带独占全屏，不显示任何侧边栏面板内容
                let is_active = if active == Route::Arrangement {
                    route == Route::Arrangement
                } else if route == Route::Automation {
                    automation_panel_visible
                } else {
                    panel_visible && route == active
                };
                item(route, icon, is_active, window, language)
            }
            RouteConfig::Toggle { icon } => toggle_item(icon, piano_roll_visible, window, language),
            RouteConfig::WaterfallToggle => waterfall_toggle_item(current_mode, window, language),
            RouteConfig::AudioExport => audio_export_item(window, language),
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

fn item<'a>(
    route: Route,
    icon_enum: icon::Icon,
    active: bool,
    window: &window::Window,
    language: Language,
) -> Element<'a> {
    let split = container(space())
        .width(2)
        .height(Length::Fill)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let background = match active {
                true => palette.primary.base.color,
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

    // 音轨总览路由使用 RouteUpdated 事件，避免触发面板切换
    // 自动化路由使用 AutomationPanelToggled 事件，独立控制
    let event = if route == Route::Arrangement {
        Event::route_updated(route)
    } else if route == Route::Automation {
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
            let palette = theme.extended_palette();
            let text_color = match status {
                Hovered | Pressed => palette.background.base.color,
                _ => palette.background.weakest.color,
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

/// 渲染独立切换按钮（如钢琴卷帘开关）
fn toggle_item<'a>(
    icon_enum: icon::Icon,
    active: bool,
    window: &window::Window,
    language: Language,
) -> Element<'a> {
    let split = container(space())
        .width(2)
        .height(Length::Fill)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let background = match active {
                true => palette.primary.base.color,
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
            let palette = theme.extended_palette();
            let text_color = match status {
                Hovered | Pressed => palette.background.base.color,
                _ => palette.background.weakest.color,
            };
            button::Style {
                text_color,
                ..Default::default()
            }
            .with_background(Color::TRANSPARENT)
        })
        .on_press(Event::piano_roll_toggled());

    widget::with_tooltip(
        btn,
        match language {
            Language::ZhCn => "钢琴卷帘",
            Language::EnUs => "Piano Roll",
        },
        iced_widget::tooltip::Position::Right,
    )
    .into()
}

/// 渲染瀑布流模式切换按钮（使用 PlayCircle SVG 图标，遵循 sidebar 按钮风格）
fn waterfall_toggle_item<'a>(
    current_mode: AppMode,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let is_waterfall = current_mode == AppMode::Waterfall;

    // 左侧激活指示条（瀑布流模式下高亮）
    let split = container(space())
        .width(2)
        .height(Length::Fill)
        .style(move |theme: &Theme| {
            let p = theme.extended_palette();
            let background = match is_waterfall {
                true => p.primary.base.color,
                false => Color::TRANSPARENT,
            };
            container::Style::default().background(background)
        });

    // PlayCircle SVG 图标
    let icon_img = icon::view_with_size_and_theme(icon::PlayCircle, 20, 20, Some(&window.theme));

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
        .on_press(Message::ModeToggled);

    let tooltip_text = match language {
        Language::ZhCn => "瀑布流播放器",
        Language::EnUs => "Waterfall Player",
    };

    widget::with_tooltip(btn, tooltip_text, iced_widget::tooltip::Position::Right).into()
}

/// 渲染音频导出按钮（使用 Download SVG 图标，遵循 sidebar 按钮风格）
fn audio_export_item<'a>(window: &'a window::Window, language: Language) -> Element<'a> {
    let icon_img = icon::view_with_size_and_theme(icon::Download, 20, 20, Some(&window.theme));

    let inner = row![
        iced_widget::Space::new().width(2),
        icon_img,
    ]
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
        .on_press(Message::AudioExport(AudioExportAction::OpenDialog));

    widget::with_tooltip(
        btn,
        match language {
            Language::ZhCn => "渲染器",
            Language::EnUs => "Renderer",
        },
        iced_widget::tooltip::Position::Right,
    )
    .into()
}
