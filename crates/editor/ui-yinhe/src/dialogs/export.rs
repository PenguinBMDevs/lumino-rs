//! 导出对话框 — yinhe `dialogs/export.rs:418` 的 iced 迁移桩
//!
//! 原 `yinhe` 含进度、完成、设置三视口；iced 桩以 `container + column + button + progress` 重建，
//! 独立窗口复用 `lumino_dialog::DialogManager`，图标走 SVG，字体走 `Theme`。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, progress_bar, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 导出进度快照（对齐 yinhe `ExportProgress` 的展示子集）
#[derive(Debug, Clone)]
pub struct ExportProgressState {
    pub progress: f32,
    pub total_duration_secs: f64,
    pub rendered_secs: f64,
    pub voice_count: usize,
    pub render_speed: f64,
    pub overall_speed: f64,
    pub status: String,
    pub visible: bool,
}

impl Default for ExportProgressState {
    fn default() -> Self {
        Self {
            progress: 0.0,
            total_duration_secs: 0.0,
            rendered_secs: 0.0,
            voice_count: 0,
            render_speed: 0.0,
            overall_speed: 0.0,
            status: String::new(),
            visible: true,
        }
    }
}

/// 导出完成快照（对齐 yinhe `ExportCompleted`）
#[derive(Debug, Clone)]
pub struct ExportCompletedState {
    pub file_path: String,
    pub elapsed_secs: f64,
    pub overall_speed: f64,
}

/// 导出设置（对齐 yinhe `WavBitDepth` 等）
#[derive(Debug, Clone)]
pub struct ExportSettingsState {
    pub bit_depth: String,
    pub export_sample_rate: u32,
    pub layer_count: u32,
    pub global_sample_rate: u32,
}

impl Default for ExportSettingsState {
    fn default() -> Self {
        Self {
            bit_depth: "16-bit".to_string(),
            export_sample_rate: 0,
            layer_count: 0,
            global_sample_rate: 48000,
        }
    }
}

fn format_duration(secs: f64) -> String {
    if secs < 0.0 {
        return "—".to_string();
    }
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// 渲染导出进度对话框
pub fn view_progress<'a>(window: &'a Window, state: &'a ExportProgressState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let grid = column![
        row![
            text("total_duration").size(12),
            iced_widget::Space::new().width(Length::Fill),
            text(format_duration(state.total_duration_secs)).size(12),
        ],
        row![
            text("rendered").size(12),
            iced_widget::Space::new().width(Length::Fill),
            text(format_duration(state.rendered_secs)).size(12),
        ],
        row![
            text("voice_count").size(12),
            iced_widget::Space::new().width(Length::Fill),
            text(format!("{}", state.voice_count)).size(12),
        ],
        row![
            text("realtime_speed").size(12),
            iced_widget::Space::new().width(Length::Fill),
            text(if state.render_speed > 0.0 {
                format!("{:.2}x", state.render_speed)
            } else {
                "—".to_string()
            })
            .size(12),
        ],
        row![
            text("overall_speed").size(12),
            iced_widget::Space::new().width(Length::Fill),
            text(if state.overall_speed > 0.0 {
                format!("{:.2}x", state.overall_speed)
            } else {
                "—".to_string()
            })
            .size(12),
        ],
    ]
    .spacing(6);

    let content = column![
        progress_bar(0.0..=1.0, state.progress),
        iced_widget::Space::new().height(10),
        grid,
        {
            let status_el: Element<'a> = if state.status.is_empty() {
                iced_widget::Space::new().height(Length::Fixed(0.0)).into()
            } else {
                text(&state.status)
                    .size(11)
                    .style(move |_t: &Theme| iced_widget::text::Style {
                        color: Some(palette.background.weak.text),
                    })
                    .into()
            };
            status_el
        },
        row![
            iced_widget::Space::new().width(Length::Fill),
            button(text("cancel").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12])
                .style(move |_t: &Theme, status| {
                    let c = match status {
                        button::Status::Hovered => palette.background.strong.color,
                        _ => palette.background.weak.color,
                    };
                    button::Style {
                        background: Some(iced_core::Background::Color(c)),
                        ..Default::default()
                    }
                }),
        ],
    ]
    .spacing(8)
    .padding(12)
    .align_x(Alignment::Center);

    container(content)
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(310.0))
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

/// 渲染导出完成对话框
pub fn view_completed<'a>(window: &'a Window, state: &'a ExportCompletedState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let content = column![
        column![
            row![
                text("elapsed").size(12),
                iced_widget::Space::new().width(Length::Fill),
                text(format_duration(state.elapsed_secs)).size(12),
            ],
            row![
                text("overall_speed").size(12),
                iced_widget::Space::new().width(Length::Fill),
                text(if state.overall_speed > 0.0 {
                    format!("{:.2}x", state.overall_speed)
                } else {
                    "—".to_string()
                })
                .size(12),
            ],
        ]
        .spacing(6),
        iced_widget::Space::new().height(8),
        row![
            iced_widget::Space::new().width(Length::Fill),
            button(text("open_folder").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12])
                .style(move |_t: &Theme, status| {
                    let c = match status {
                        button::Status::Hovered => palette.primary.strong.color,
                        _ => palette.primary.base.color,
                    };
                    button::Style {
                        background: Some(iced_core::Background::Color(c)),
                        text_color: iced_core::Color::WHITE,
                        ..Default::default()
                    }
                }),
            button(text("ok").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12]),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .padding(16);

    container(content)
        .width(Length::Fixed(320.0))
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

/// 渲染导出设置对话框
pub fn view_settings<'a>(window: &'a Window, state: &'a ExportSettingsState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let sr_text = if state.export_sample_rate == 0 {
        format!("follow_global {}", state.global_sample_rate)
    } else {
        format!("{} Hz", state.export_sample_rate)
    };

    let content = column![
        row![
            text("bit_depth").size(12),
            iced_widget::Space::new().width(Length::Fill),
            container(text(&state.bit_depth).size(12))
                .padding(6)
                .style(move |_t: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(palette.background.weak.color)),
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        ]
        .align_y(Alignment::Center),
        row![
            text("sample_rate").size(12),
            iced_widget::Space::new().width(Length::Fill),
            text(sr_text).size(12),
        ]
        .align_y(Alignment::Center),
        row![
            text("xsynth_layers").size(12),
            iced_widget::Space::new().width(Length::Fill),
            text(if state.layer_count == 0 {
                "unlimited".to_string()
            } else {
                format!("{}", state.layer_count)
            })
            .size(12),
        ]
        .align_y(Alignment::Center),
        iced_widget::Space::new().height(8),
        row![
            iced_widget::Space::new().width(Length::Fill),
            button(text("start").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 14])
                .style(move |_t: &Theme, status| {
                    let c = match status {
                        button::Status::Hovered => palette.primary.strong.color,
                        _ => palette.primary.base.color,
                    };
                    button::Style {
                        background: Some(iced_core::Background::Color(c)),
                        text_color: iced_core::Color::WHITE,
                        ..Default::default()
                    }
                }),
        ],
    ]
    .spacing(10)
    .padding(16);

    container(content)
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(220.0))
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
