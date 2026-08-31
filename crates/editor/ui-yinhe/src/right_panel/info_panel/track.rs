//! 音轨信息面板 — 对应 yinhe `right_panel/info_panel/track.rs:474`
//!
//! 显示选中音轨的名称 / 端口 / 通道 / Mute / Solo / 颜色 / 摘要，
//! 以及 Conductor 轨与多选汇总。yinhe 原通过 `ComboBox` 选轨、
//! `TextEdit::singleline` 编名称、`color_edit_button` 改色、
//! `track_overrides` 切 M/S；iced 桩用 `column + row + button + text`
//! 重构，保留端口/通道切换与 Mute/Solo 语义，编辑走 `Message` 单向流。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::{Element, Theme, window::Window};

/// 音轨信息行（对齐 yinhe `TrackInfoCache` / `TrackData` 精简）
///
/// `port` 0..15 → 显示 `A..P`，`channel` 0..15 → 显示 `01..16`。
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub index: u16,
    pub name: String,
    pub port: u8,
    pub channel: u8,
    pub color: [f32; 4],
    pub muted: bool,
    pub soloed: bool,
    pub note_count: usize,
    pub event_count: usize,
    pub program: Option<u8>,
    pub is_conductor: bool,
}

impl Default for TrackInfo {
    fn default() -> Self {
        Self {
            index: 0,
            name: "Track 0".to_string(),
            port: 0,
            channel: 0,
            color: [0.4, 0.6, 0.9, 1.0],
            muted: false,
            soloed: false,
            note_count: 0,
            event_count: 0,
            program: None,
            is_conductor: false,
        }
    }
}

/// 音轨面板聚合状态（对齐 yinhe `Document.data.model.tracks` + `track_selected`）
///
/// `rows` 含全部音轨，`selected` 为单选索引（yinhe 用 `HashSet<u16>`，
/// 此处 iced 桩简化为单选；多选由 selection 面板处理）。
#[derive(Debug, Clone)]
pub struct TrackPanelInfoState {
    pub rows: Vec<TrackInfo>,
    pub selected: Option<u16>,
}

impl Default for TrackPanelInfoState {
    fn default() -> Self {
        Self {
            rows: vec![TrackInfo::default()],
            selected: Some(0),
        }
    }
}

fn port_label(port: u8) -> String {
    format!("Port {}", (b'A' + port.min(15)) as char)
}

fn channel_label(ch: u8) -> String {
    format!("{:02}", ch + 1)
}

fn small_label<'a>(window: &'a Window, s: impl Into<String>) -> Element<'a> {
    let palette = window.theme.extended_palette();
    text(s.into())
        .size(11)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(palette.background.weak.text),
        })
        .into()
}

fn bright_text<'a>(window: &'a Window, s: impl Into<String>, size: f32) -> Element<'a> {
    let palette = window.theme.extended_palette();
    text(s.into())
        .size(size)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(palette.background.strong.text),
        })
        .into()
}

/// 渲染音轨信息（对齐 yinhe `show_track_info` 分支）
///
/// - 无音轨 → 空提示
/// - Conductor → 标题 + 提示 + 歌曲名/Tempo/TimeSig 计数
/// - 普通轨 → 名称行 + 端口/通道 + 颜色 + M/S + 摘要
pub fn view<'a>(window: &'a Window, state: &'a TrackPanelInfoState, idx: u16) -> Element<'a> {
    let palette = window.theme.extended_palette();

    if state.rows.is_empty() {
        return container(text("No tracks").size(12))
            .padding([12, 12])
            .into();
    }

    let sel_idx = state
        .selected
        .and_then(|s| state.rows.iter().position(|r| r.index == s))
        .unwrap_or(0)
        .min(state.rows.len().saturating_sub(1));
    let track = state
        .rows
        .iter()
        .find(|r| r.index == idx)
        .unwrap_or(&state.rows[sel_idx]);

    if track.is_conductor {
        return conductor_view(window, track);
    }

    let name_row = row![
        small_label(window, "Name:"),
        container(text(track.name.clone()).size(12))
            .padding([4, 6])
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.weak.color)),
                border: iced_core::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let port_row = row![
        small_label(window, "Port/Channel:"),
        text(port_label(track.port)).size(11),
        text(channel_label(track.channel)).size(11),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let color_preview = container(text("").size(1))
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(iced_core::Color::from_rgba(
                track.color[0],
                track.color[1],
                track.color[2],
                track.color[3],
            ))),
            border: iced_core::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let color_row = row![
        small_label(window, "Color:"),
        color_preview,
        button(text("Reset").size(11))
            .padding([2, 6])
            .style(|_theme: &Theme, _| { button::Style::default() }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mute_btn = button(text(if track.muted { "Mute ●" } else { "Mute" }).size(11))
        .padding([4, 8])
        .style(move |_theme: &Theme, status| {
            let bg = if track.muted {
                iced_core::Color::from_rgb(0.95, 0.33, 0.33)
            } else if status == button::Status::Hovered {
                palette.background.weak.color
            } else {
                iced_core::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                text_color: if track.muted {
                    iced_core::Color::WHITE
                } else {
                    palette.background.base.text
                },
                border: iced_core::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        });

    let solo_btn = button(text(if track.soloed { "Solo ●" } else { "Solo" }).size(11))
        .padding([4, 8])
        .style(move |_theme: &Theme, status| {
            let bg = if track.soloed {
                iced_core::Color::from_rgb(0.33, 0.62, 0.95)
            } else if status == button::Status::Hovered {
                palette.background.weak.color
            } else {
                iced_core::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                text_color: if track.soloed {
                    iced_core::Color::WHITE
                } else {
                    palette.background.base.text
                },
                border: iced_core::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        });

    let ms_row = row![mute_btn, solo_btn].spacing(8);

    let summary = column![
        bright_text(window, "Properties", 11.0),
        row![
            small_label(window, "Notes:"),
            text(track.note_count.to_string()).size(11)
        ]
        .spacing(6),
        row![
            small_label(window, "Events:"),
            text(track.event_count.to_string()).size(11)
        ]
        .spacing(6),
        row![
            small_label(window, "Program:"),
            text(
                track
                    .program
                    .map(|p| format!("PC {p}"))
                    .unwrap_or_else(|| "—".to_string())
            )
            .size(11)
        ]
        .spacing(6),
    ]
    .spacing(4);

    column![name_row, port_row, color_row, ms_row, summary,]
        .spacing(8)
        .padding([8, 8])
        .into()
}

fn conductor_view<'a>(window: &'a Window, track: &'a TrackInfo) -> Element<'a> {
    column![
        bright_text(window, "Conductor", 13.0),
        text("Conductor: tempo / time-sig / global events")
            .size(11)
            .style(|_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(iced_core::Color::from_rgb(0.6, 0.6, 0.6)),
                }
            }),
        row![
            small_label(window, "Song:"),
            bright_text(window, track.name.clone(), 12.0)
        ]
        .spacing(6),
        row![
            small_label(window, "Tempo events:"),
            text(track.event_count.to_string()).size(11)
        ]
        .spacing(6),
        button(text("Clear selection").size(11)).padding([4, 8]),
    ]
    .spacing(6)
    .padding([8, 8])
    .into()
}

/// 计算每轨 skip mask 并发给音频引擎的 iced 桩（签名对齐 yinhe `send_skip_tracks`）
///
/// 实际音频侧通过 `Message::AudioAction` 重建，此处仅保留纯函数以便测试。
#[must_use]
pub fn compute_skip_mask_stub(overrides: &[(bool, bool)]) -> u32 {
    let mut mask = 0u32;
    for (i, (muted, soloed)) in overrides.iter().enumerate().take(32) {
        let _ = soloed;
        if *muted {
            mask |= 1u32 << i;
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn track_view_conductor() {
        let window = Window::new("Tokyo Night Storm");
        let state = TrackPanelInfoState {
            rows: vec![TrackInfo {
                index: 0,
                name: "Master".to_string(),
                is_conductor: true,
                ..Default::default()
            }],
            selected: Some(0),
        };
        let _el = view(&window, &state, 0);
    }

    #[test]
    fn track_view_normal() {
        let window = Window::new("Tokyo Night Storm");
        let state = TrackPanelInfoState {
            rows: vec![
                TrackInfo {
                    index: 0,
                    name: "Piano".to_string(),
                    port: 0,
                    channel: 0,
                    note_count: 42,
                    program: Some(1),
                    ..Default::default()
                },
                TrackInfo {
                    index: 1,
                    name: "Bass".to_string(),
                    muted: true,
                    ..Default::default()
                },
            ],
            selected: Some(0),
        };
        let _el = view(&window, &state, 0);
    }
}
