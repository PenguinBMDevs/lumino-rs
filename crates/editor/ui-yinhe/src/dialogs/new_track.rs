//! 新建音轨对话框 — yinhe `dialogs/new_track.rs:406` 的 iced 迁移桩
//!
//! 原 `egui` 实现含通道分配预览（`yinhe_editor_core::channel_alloc`）；
//! iced 桩以 `container + column + button + pick_list` 重建，独立窗口复用
//! `DialogManager`，图标/字体走 `Theme`。

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, pick_list, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

const MAX_COUNT: usize = 64;

/// 音轨种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindChoice {
    Midi,
    Instrument,
}

impl std::fmt::Display for KindChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Midi => write!(f, "MIDI"),
            Self::Instrument => write!(f, "Instrument"),
        }
    }
}

/// 分配方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignMode {
    Auto,
    Manual,
}

impl std::fmt::Display for AssignMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "Auto"),
            Self::Manual => write!(f, "Manual"),
        }
    }
}

/// 新建音轨对话框状态
#[derive(Debug, Clone)]
pub struct NewTrackDialogState {
    pub open: bool,
    pub kind: KindChoice,
    pub count: usize,
    pub mode: AssignMode,
    pub manual_port: u8,
    pub manual_channel: u8,
    pub manual_instrument: usize,
    pub preview: String,
    pub error: Option<String>,
}

impl Default for NewTrackDialogState {
    fn default() -> Self {
        Self {
            open: false,
            kind: KindChoice::Midi,
            count: 1,
            mode: AssignMode::Auto,
            manual_port: 0,
            manual_channel: 0,
            manual_instrument: 1,
            preview: String::new(),
            error: None,
        }
    }
}

impl NewTrackDialogState {
    pub fn open(&mut self) {
        *self = Self {
            open: true,
            ..Self::default()
        };
    }
}

fn midi_badge(port: u8, channel: u8) -> String {
    format!("{}{:02}", (b'A' + port.min(15)) as char, channel + 1)
}

/// 渲染新建音轨对话框
pub fn view<'a>(window: &'a Window, state: &'a NewTrackDialogState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    let kind_row = row![
        text("kind").size(12),
        pick_list(
            [KindChoice::Midi, KindChoice::Instrument],
            Some(state.kind),
            |_| lumino_ui_core::message::null()
        )
        .placeholder("kind")
        .padding(6),
        container(
            text("Audio")
                .size(12)
                .style(move |_t: &Theme| iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                })
        )
        .padding(6)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(weak.scale_alpha(0.5))),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let count_row = row![
        text("count").size(12),
        button(text("-").size(12))
            .on_press(lumino_ui_core::message::null())
            .padding(4),
        text(format!("{}", state.count)).size(12),
        button(text("+").size(12))
            .on_press_maybe((state.count < MAX_COUNT).then_some(lumino_ui_core::message::null()))
            .padding(4),
        text(format!("1..={MAX_COUNT}"))
            .size(10)
            .style(move |_t: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }
            }),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let assign_row = row![
        text("assign").size(12),
        pick_list(
            [AssignMode::Auto, AssignMode::Manual],
            Some(state.mode),
            |_| lumino_ui_core::message::null()
        )
        .placeholder("assign")
        .padding(6),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let manual_row: Element<'a> = if state.mode == AssignMode::Manual {
        match state.kind {
            KindChoice::Midi => row![
                text("port").size(12),
                pick_list(
                    (0u8..16).collect::<Vec<_>>(),
                    Some(state.manual_port),
                    |_| lumino_ui_core::message::null()
                )
                .placeholder("port")
                .padding(4),
                text(format!("({})", (b'A' + state.manual_port.min(15)) as char)).size(11),
                text("channel").size(12),
                pick_list(
                    (0u8..16).collect::<Vec<_>>(),
                    Some(state.manual_channel),
                    |_| lumino_ui_core::message::null()
                )
                .placeholder("ch")
                .padding(4),
                text(format!("{}", state.manual_channel + 1)).size(11),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
            KindChoice::Instrument => row![
                text("instrument_start").size(12),
                text(format!("{}", state.manual_instrument)).size(12),
                button(text("-").size(12))
                    .on_press(lumino_ui_core::message::null())
                    .padding(4),
                button(text("+").size(12))
                    .on_press(lumino_ui_core::message::null())
                    .padding(4),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        }
    } else {
        iced_widget::Space::new().height(0).into()
    };

    let preview = text(format!("preview: {}", state.preview))
        .size(11)
        .style(move |_t: &Theme| iced_widget::text::Style {
            color: Some(palette.background.weak.text),
        });

    let error_row: Element<'a> = if let Some(err) = &state.error {
        text(err)
            .size(11)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.danger.base.color),
            })
            .into()
    } else {
        iced_widget::Space::new().height(0).into()
    };

    // 通道预览 badge 行（按数量生成占位）
    let badges: Vec<Element<'a>> = (0..state.count.min(8))
        .map(|i| {
            let badge = match state.kind {
                KindChoice::Midi => {
                    let port = state.manual_port;
                    let ch = (state.manual_channel as usize + i) % 16;
                    midi_badge(port, ch as u8)
                }
                KindChoice::Instrument => {
                    format!("I{:02}", state.manual_instrument + i)
                }
            };
            container(text(badge).size(10))
                .padding([2, 6])
                .style(move |_t: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(weak)),
                    border: iced_core::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .into()
        })
        .collect();

    let content = column![
        kind_row,
        count_row,
        assign_row,
        manual_row,
        row(badges).spacing(4),
        preview,
        error_row,
        iced_widget::Space::new().height(8),
        row![
            iced_widget::Space::new().width(Length::Fill),
            button(text("cancel").size(12))
                .on_press(lumino_ui_core::message::null())
                .padding([6, 12]),
            button(text("confirm").size(12))
                .on_press_maybe(
                    state
                        .error
                        .is_none()
                        .then_some(lumino_ui_core::message::null())
                )
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
        ]
        .spacing(8),
    ]
    .spacing(10)
    .padding(16);

    container(content)
        .width(Length::Fixed(400.0))
        .height(Length::Fixed(320.0))
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
