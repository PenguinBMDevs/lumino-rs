//! 音色库面板 — 对应 yinhe `right_panel/soundfont.rs:273`
//!
//! 顶部分全局/工程二选一，下方按模式显示列表：
//! - 全局：单列表 `ports[0]`（所有 port 共享）
//! - 工程：按 port 选择的 `overrides` 列表（`soundfont_selected_port`）
//! yinhe 原用 `ComboBox` 选 port、`FileDialog` 添加、`sf_list` 渲染；
//! iced 桩以 `column + row + button + sf_list::view` 重构，保留二态与统计栏。

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::{Element, Theme, window::Window};

use super::sf_list::{SfEntry, SfListState, view as sf_list_view};

/// 音色库模式（对齐 yinhe `global_sf_config.global_enabled`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SoundfontTab {
    #[default]
    Global,
    Project,
}

/// 音色库面板状态（对齐 yinhe `AudioSettings.global_sf_config` + `Document.edit.project_sf` 精简）
#[derive(Debug, Clone, Default)]
pub struct SoundfontPanelState {
    pub tab: SoundfontTab,
    pub global_entries: Vec<SfEntry>,
    pub project_entries: Vec<(u8, Vec<SfEntry>)>,
    pub selected_port: u8,
    pub ports: Vec<u8>,
}

impl SoundfontPanelState {
    #[must_use]
    pub fn global_total(&self) -> usize {
        self.global_entries.len()
    }

    #[must_use]
    pub fn global_enabled(&self) -> usize {
        self.global_entries.iter().filter(|e| e.enabled).count()
    }

    #[must_use]
    pub fn project_total(&self) -> usize {
        self.project_entries.iter().map(|(_, v)| v.len()).sum()
    }

    #[must_use]
    pub fn project_enabled(&self) -> usize {
        self.project_entries
            .iter()
            .flat_map(|(_, v)| v.iter())
            .filter(|e| e.enabled)
            .count()
    }

    #[must_use]
    pub fn port_entries(&self, port: u8) -> Option<&[SfEntry]> {
        self.project_entries
            .iter()
            .find(|(p, _)| *p == port)
            .map(|(_, v)| v.as_slice())
    }
}

fn tab_button<'a>(window: &'a Window, label: &'a str, active: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    button(text(label).size(12))
        .padding([4, 10])
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

/// 音色库面板 `view()` — 顶 Tab + 列表 + 底统计栏
pub fn view<'a>(
    window: &'a Window,
    state: &'a SoundfontPanelState,
    sf_state: &'a SfListState,
) -> Element<'a> {
    let palette = window.theme.extended_palette();

    let tabs = row![
        tab_button(window, "Global", state.tab == SoundfontTab::Global),
        tab_button(window, "Project", state.tab == SoundfontTab::Project),
    ]
    .spacing(8)
    .padding([4, 4]);

    let content: Element<'a> = match state.tab {
        SoundfontTab::Global => {
            let hint = text("Global SoundFonts — shared by all ports")
                .size(11)
                .style(move |_theme: &Theme| iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                });
            let list = sf_list_view(window, &state.global_entries, sf_state);
            column![hint, list].spacing(6).into()
        }
        SoundfontTab::Project => {
            if state.ports.is_empty() {
                container(text("No ports — open a document").size(11))
                    .padding([12, 12])
                    .into()
            } else {
                let port_label = format!("Port {}", (b'A' + state.selected_port.min(15)) as char);
                let entries = state.port_entries(state.selected_port).unwrap_or(&[]);
                let list = sf_list_view(window, entries, sf_state);
                column![
                    row![
                        text("Port:").size(11),
                        container(text(port_label).size(11)).padding([2, 6]),
                        button(text("Add for port").size(11)).padding([4, 8]),
                    ]
                    .spacing(8),
                    list,
                ]
                .spacing(6)
                .into()
            }
        }
    };

    let status: Element<'a> = match state.tab {
        SoundfontTab::Global => text(format!(
            "Global: {}/{} enabled",
            state.global_enabled(),
            state.global_total()
        ))
        .size(10)
        .into(),
        SoundfontTab::Project => text(format!(
            "Project: {}/{} enabled",
            state.project_enabled(),
            state.project_total()
        ))
        .size(10)
        .into(),
    };

    let footer = container(
        row![
            status,
            iced_widget::space::horizontal().width(Length::Fill),
            button(text("Reload audio").size(11)).padding([4, 8]),
        ]
        .align_y(iced_core::Alignment::Center),
    )
    .padding([6, 6])
    .style(move |_theme: &Theme| container::Style {
        background: Some(iced_core::Background::Color(palette.background.weak.color)),
        ..Default::default()
    });

    column![tabs, content, footer,]
        .spacing(4)
        .padding([4, 4])
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn soundfont_view_global_and_project() {
        let window = Window::new("Tokyo Night Storm");
        let sf_state = SfListState::default();
        let mut state = SoundfontPanelState {
            tab: SoundfontTab::Global,
            global_entries: vec![SfEntry::new("/a/b.sf2".to_string(), "Grand".to_string())],
            ..Default::default()
        };
        let _ = view(&window, &state, &sf_state);
        state.tab = SoundfontTab::Project;
        state.ports = vec![0, 1];
        state.project_entries = vec![(
            0,
            vec![SfEntry::new("/x.sfz".to_string(), "Str".to_string())],
        )];
        let _ = view(&window, &state, &sf_state);
    }
}
