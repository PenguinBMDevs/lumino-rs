//! 设置对话框 — yinhe `dialogs/settings.rs:255` + `dialogs/settings/*` 的 iced 迁移桩
//!
//! 原 `egui` 实现为左右分栏（左侧搜索 + 分类导航，右侧滚动内容）；
//! iced 桩以 `container + column + row + scrollable + button + text_input` 重建，
//! 独立窗口复用 `DialogManager`，图标/字体走 `Theme`。

pub mod audio;
pub mod constants;
pub mod general;
pub mod language;
pub mod render;
pub mod search;
pub mod shortcuts;
pub mod theme;

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, scrollable, text, text_input};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 设置标签页（与 `constants::CATEGORY_KEYS` 索引一一对应）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Theme,
    Language,
    Audio,
    Render,
    Shortcuts,
    General,
}

/// 设置对话框视图状态（由 Host 持有，受控组件）
#[derive(Debug, Clone, Default)]
pub struct SettingsDialogState {
    pub tab: SettingsTab,
    pub search_query: String,
}

impl SettingsTab {
    pub fn all() -> [Self; 6] {
        [
            Self::Theme,
            Self::Language,
            Self::Audio,
            Self::Render,
            Self::Shortcuts,
            Self::General,
        ]
    }

    pub fn label_key(self) -> &'static str {
        constants::CATEGORY_KEYS[self as usize]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Language => "language",
            Self::Audio => "audio",
            Self::Render => "render",
            Self::Shortcuts => "shortcuts",
            Self::General => "general",
        }
    }
}

fn category_button<'a>(window: &'a Window, label: &'static str, selected: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = if selected {
        palette.background.strong.color
    } else {
        iced_core::Color::TRANSPARENT
    };
    let txt_color = if selected {
        palette.background.strong.text
    } else {
        palette.background.base.text
    };
    button(
        text(label)
            .size(12)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(txt_color),
            }),
    )
    .on_press(lumino_ui_core::message::null())
    .width(Length::Fill)
    .padding([6, 10])
    .style(move |_t: &Theme, _| button::Style {
        background: Some(iced_core::Background::Color(bg)),
        border: iced_core::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// 渲染设置对话框主体（左右分栏）
pub fn view<'a>(window: &'a Window, state: &'a SettingsDialogState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    // 左侧：搜索 + 分类导航
    let clear_btn: Element<'a> = if state.search_query.is_empty() {
        iced_widget::Space::new().width(Length::Fixed(0.0)).into()
    } else {
        button(text("clear").size(10))
            .on_press(lumino_ui_core::message::null())
            .padding(4)
            .into()
    };
    let search_row = row![
        text_input("search", &state.search_query)
            .on_input(|_| lumino_ui_core::message::null())
            .padding(6)
            .size(12),
        clear_btn,
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let nav: Vec<Element<'a>> = SettingsTab::all()
        .iter()
        .map(|tab| category_button(window, tab.label(), *tab == state.tab))
        .collect();

    let left = container(
        column![search_row, column(nav).spacing(2)]
            .spacing(8)
            .padding(8),
    )
    .width(Length::Fixed(132.0))
    .height(Length::Fill)
    .style(move |_t: &Theme| container::Style {
        background: Some(iced_core::Background::Color(weak.scale_alpha(0.25))),
        ..Default::default()
    });

    // 右侧：根据搜索/标签页切换内容
    let right_content: Element<'a> = if !state.search_query.trim().is_empty() {
        search::view(window, &state.search_query)
    } else {
        match state.tab {
            SettingsTab::Theme => theme::view(window),
            SettingsTab::Language => language::view(window),
            SettingsTab::Audio => audio::view(window),
            SettingsTab::Render => render::view(window),
            SettingsTab::Shortcuts => shortcuts::view(window),
            SettingsTab::General => general::view(window),
        }
    };

    let right = container(scrollable(right_content).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(8);

    let divider = container(
        iced_widget::Space::new()
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fixed(1.0))
    .height(Length::Fill)
    .style(move |_t: &Theme| container::Style {
        background: Some(iced_core::Background::Color(weak)),
        ..Default::default()
    });
    let body = row![left, divider, right].spacing(0).height(Length::Fill);

    container(body)
        .width(Length::Fixed(760.0))
        .height(Length::Fixed(620.0))
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
