//! 压缩包选择对话框 — yinhe `dialogs/archive_picker.rs:631` 的 iced 迁移桩
//!
//! 原 `egui` 实现含异步打开（`mpsc::Receiver`）、搜索过滤、列表选中与二次确认；
//! iced 桩以 `container + column + button + scrollable + text_input` 重建骨架，
//! 独立窗口复用 `lumino_dialog::DialogManager`（每个对话框为独立 `DialogWindow`），
//! 图标走 `lumino_ui_core::resources::icon` SVG，字体/配色走 `Theme`。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, scrollable, text, text_input};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 压缩包条目（对齐 `yinhe_archive::ArchiveEntry` 的展示子集）
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
}

/// 归档选择器状态（对齐 yinhe `ArchivePicker`）
#[derive(Debug, Clone)]
pub struct ArchivePickerState {
    pub path: String,
    pub entries: Vec<ArchiveEntry>,
    pub selected_idx: Option<usize>,
    pub search_query: String,
    pub is_opening: bool,
}

impl Default for ArchivePickerState {
    fn default() -> Self {
        Self {
            path: String::new(),
            entries: Vec::new(),
            selected_idx: None,
            search_query: String::new(),
            is_opening: false,
        }
    }
}

impl ArchivePickerState {
    fn filtered_indices(&self) -> Vec<usize> {
        let q = self.search_query.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| q.is_empty() || e.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }
}

fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let truncated: String = name.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}…")
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 密码输入提示状态（对齐 yinhe `PasswordPrompt`）
#[derive(Debug, Clone, Default)]
pub struct PasswordPromptState {
    pub path: String,
    pub password: String,
    pub wrong: bool,
    pub show_password: bool,
}

/// 渲染压缩包选择对话框
pub fn view<'a>(window: &'a Window, state: &'a ArchivePickerState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;
    let strong = palette.background.strong.color;

    if state.is_opening {
        let spinner = container(
            column![
                text("scanning").size(14),
                text(&state.path).size(11).style(move |_t: &Theme| {
                    iced_widget::text::Style {
                        color: Some(palette.background.weak.text),
                    }
                }),
            ]
            .spacing(8)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        });
        return spinner.into();
    }

    let filename = std::path::Path::new(&state.path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| state.path.clone());
    let display_name = truncate_name(&filename, 45);

    let header = column![
        text(format!("source: {display_name}"))
            .size(14)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.background.base.text),
            }),
        row![
            lumino_ui_core::resources::icon::view_with_size_and_theme(
                lumino_ui_core::resources::icon::Icon::Gear,
                14,
                14,
                Some(&window.theme)
            ),
            text_input("search", &state.search_query)
                .on_input(|_| lumino_ui_core::message::null())
                .padding(6)
                .size(13),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(6);

    let filtered = state.filtered_indices();
    let rows: Vec<Element<'a>> = filtered
        .iter()
        .map(|&idx| {
            let entry = &state.entries[idx];
            let is_selected = state.selected_idx == Some(idx);
            let display = truncate_name(&entry.name, 55);
            let size_text = format_size(entry.size);
            let row_bg = if is_selected {
                strong
            } else {
                iced_core::Color::TRANSPARENT
            };
            let prefix = if is_selected { "▶ " } else { "  " };
            container(
                row![
                    text(format!("{prefix}{display}")).size(12),
                    iced_widget::Space::new().width(Length::Fill),
                    text(size_text).size(11).style(move |_t: &Theme| {
                        iced_widget::text::Style {
                            color: Some(palette.background.weak.text),
                        }
                    }),
                ]
                .align_y(Alignment::Center)
                .padding([4, 8]),
            )
            .width(Length::Fill)
            .style(move |_t: &Theme| container::Style {
                background: Some(iced_core::Background::Color(row_bg)),
                border: iced_core::Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        })
        .collect();

    let list = container(scrollable(column(rows).spacing(2)).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(240.0))
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(weak.scale_alpha(0.35))),
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: weak,
            },
            ..Default::default()
        });

    let footer = row![
        text(format!("{} files", filtered.len()))
            .size(11)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }),
        iced_widget::Space::new().width(Length::Fill),
        button(text("cancel").size(12))
            .on_press(lumino_ui_core::message::null())
            .padding([6, 12])
            .style(move |_t: &Theme, status| {
                let bg_col = match status {
                    button::Status::Hovered => weak,
                    _ => iced_core::Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(iced_core::Background::Color(bg_col)),
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }),
        button(text("confirm").size(12))
            .on_press_maybe(
                state
                    .selected_idx
                    .is_some()
                    .then_some(lumino_ui_core::message::null()),
            )
            .padding([6, 14])
            .style(move |_t: &Theme, status| {
                let bg_col = match status {
                    button::Status::Hovered => palette.primary.strong.color,
                    _ => palette.primary.base.color,
                };
                button::Style {
                    background: Some(iced_core::Background::Color(bg_col)),
                    text_color: iced_core::Color::WHITE,
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let content = column![header, list, footer]
        .spacing(10)
        .padding(12)
        .width(Length::Fill);

    container(content)
        .width(Length::Fixed(560.0))
        .height(Length::Fixed(400.0))
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

/// 渲染密码输入对话框
pub fn view_password<'a>(window: &'a Window, state: &'a PasswordPromptState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let filename = std::path::Path::new(&state.path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| state.path.clone());
    let display = truncate_name(&filename, 40);

    let wrong_row: Element<'a> = if state.wrong {
        text("password_wrong")
            .size(12)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.danger.base.color),
            })
            .into()
    } else {
        iced_widget::Space::new().height(0).into()
    };

    let content = column![
        text(format!("password for {display}")).size(13),
        wrong_row,
        row![
            text_input("password", &state.password)
                .on_input(|_| lumino_ui_core::message::null())
                .secure(!state.show_password)
                .padding(8),
            button(lumino_ui_core::resources::icon::view_with_size_and_theme(
                if state.show_password {
                    lumino_ui_core::resources::icon::Icon::EyeSlash
                } else {
                    lumino_ui_core::resources::icon::Icon::Eye
                },
                16,
                16,
                Some(&window.theme)
            ))
            .on_press(lumino_ui_core::message::null())
            .padding(6)
            .style(|_t: &Theme, _| button::Style::default()),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        row![
            iced_widget::Space::new().width(Length::Fill),
            button(text("cancel").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12]),
            button(text("confirm").size(12))
                .on_press_maybe(
                    (!state.password.is_empty()).then_some(lumino_ui_core::message::null())
                )
                .padding([6, 12])
                .style(move |_t: &Theme, status| {
                    let bg_col = match status {
                        button::Status::Hovered => palette.primary.strong.color,
                        _ => palette.primary.base.color,
                    };
                    button::Style {
                        background: Some(iced_core::Background::Color(bg_col)),
                        text_color: iced_core::Color::WHITE,
                        ..Default::default()
                    }
                }),
        ]
        .spacing(8),
    ]
    .spacing(10)
    .padding(16);

    container(content)
        .width(Length::Fixed(460.0))
        .height(Length::Fixed(160.0))
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
