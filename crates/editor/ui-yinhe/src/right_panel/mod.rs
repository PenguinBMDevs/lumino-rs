//! 右侧面板 — yinhe `right_panel` 16 文件的 iced 迁移桩
//!
//! 对应 yinhe 原：
//! - `info_panel/track.rs:474` + `selection.rs:724` + `anchor.rs:435`
//! - `event_browser/tree.rs:507` + `table.rs:554` + `detail.rs:1243`
//!   + `edit/*6` + `edit_ops.rs:558` + `state.rs:172` + `bar_lookup.rs:156`
//! - `sf_list.rs:379` + `soundfont.rs:273` + `project_info.rs:201`
//!
//! iced 约束：
//! - 全部走 `lumino_ui_core::{Theme, Element}`，不引入 `egui` / `egui_extras`
//! - `event_browser::table` 用 `scrollable` 虚拟化（分页 + `scrollable`），
//!   `tree` 用 `column + button`
//! - 主题色经 `Window.theme.extended_palette()` 获取

pub mod event_browser;
pub mod info_panel;
pub mod project_info;
pub mod sf_list;
pub mod soundfont;

pub use event_browser::{EventBrowserState, JumpRequest};
pub use info_panel::{InfoContent, InfoPanelState};
pub use project_info::ProjectInfoState;
pub use sf_list::{SfEntry, SfListState};
pub use soundfont::{SoundfontPanelState, SoundfontTab};

use iced_core::Length;
use iced_widget::{column, container, row, text};

use lumino_ui_core::{Element, Theme, window::Window};

/// 右侧面板 Tab（与 yinhe 右侧标签一一对应）
///
/// yinhe 原 `right_panel` 在 `TopBar` 切换 Info / EventBrowser / SoundFont，
/// 无文档时回落 `project_info`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightPanelTab {
    #[default]
    Info,
    EventBrowser,
    SoundFont,
    ProjectInfo,
}

impl RightPanelTab {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::EventBrowser => "Events",
            Self::SoundFont => "SoundFont",
            Self::ProjectInfo => "Project",
        }
    }

    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Info,
            Self::EventBrowser,
            Self::SoundFont,
            Self::ProjectInfo,
        ]
    }
}

/// 右侧面板聚合状态（供上层 `Host/Root` 持有）
///
/// 对齐 yinhe `Document.edit` 中分散的 `info_content` / `EventBrowserState` /
/// `soundfont_selected_port` / `project_sf` 等字段的 iced 侧聚合。
#[derive(Debug, Clone)]
pub struct RightPanelState {
    pub tab: RightPanelTab,
    pub info: InfoPanelState,
    pub event_browser: EventBrowserState,
    pub soundfont: SoundfontPanelState,
    pub project: ProjectInfoState,
    pub sf_list: SfListState,
}

impl Default for RightPanelState {
    fn default() -> Self {
        Self {
            tab: RightPanelTab::Info,
            info: InfoPanelState::default(),
            event_browser: EventBrowserState::default(),
            soundfont: SoundfontPanelState::default(),
            project: ProjectInfoState::default(),
            sf_list: SfListState::default(),
        }
    }
}

/// 右侧面板入口 `view()` — 纵向 Tab 栏 + 内容区
///
/// ```text
/// column![
///   row![ tab_btn("Info"), tab_btn("Events"), ... ],
///   container( content_for(tab) )
/// ]
/// ```
pub fn view<'a>(window: &'a Window, state: &'a RightPanelState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let tabs = row(RightPanelTab::all()
        .iter()
        .map(|t| tab_button(window, *t, state.tab == *t))
        .collect::<Vec<Element<'a>>>())
    .spacing(4)
    .padding([4, 6]);

    let content: Element<'a> = match state.tab {
        RightPanelTab::Info => info_panel::view(window, &state.info),
        RightPanelTab::EventBrowser => event_browser::view(window, &state.event_browser),
        RightPanelTab::SoundFont => soundfont::view(window, &state.soundfont, &state.sf_list),
        RightPanelTab::ProjectInfo => project_info::view(window, &state.project),
    };

    let body = column![tabs, content].spacing(0);

    container(body)
        .width(Length::Fixed(240.0))
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                color: palette.background.strong.color,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn tab_button<'a>(window: &'a Window, tab: RightPanelTab, active: bool) -> Element<'a> {
    use iced_widget::button;

    let palette = window.theme.extended_palette();
    let label = text(tab.label()).size(11);

    button(label)
        .padding([4, 8])
        .style(move |_theme: &Theme, status| {
            let bg = if active {
                palette.background.strong.color
            } else if status == button::Status::Hovered {
                palette.background.weak.color
            } else {
                iced_core::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                text_color: if active {
                    palette.background.strong.text
                } else {
                    palette.background.base.text
                },
                border: iced_core::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn right_panel_view_does_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let state = RightPanelState::default();
        let _el = view(&window, &state);
    }

    #[test]
    fn right_panel_tabs_switch() {
        let window = Window::new("Tokyo Night Storm");
        for tab in RightPanelTab::all() {
            let mut state = RightPanelState::default();
            state.tab = *tab;
            let _el = view(&window, &state);
        }
    }
}
