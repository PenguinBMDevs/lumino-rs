//! 音频设置页 — yinhe `dialogs/settings/audio.rs:190` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, checkbox, column, container, pick_list, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    let devices = ["Default Device", "Device A", "Device B"];
    let sample_rates = [44100, 48000, 96000];

    let content = column![
        text("audio").size(14),
        row![
            text("output_device").size(12).width(Length::Fixed(120.0)),
            pick_list(devices.to_vec(), Some("Default Device"), |_| {
                lumino_ui_core::message::null()
            })
            .placeholder("device")
            .padding(6),
        ]
        .spacing(8),
        row![
            text("midi_input_device")
                .size(12)
                .width(Length::Fixed(120.0)),
            pick_list(vec!["None", "MIDI A"], Some("None"), |_| {
                lumino_ui_core::message::null()
            })
            .placeholder("midi")
            .padding(6),
        ]
        .spacing(8),
        row![
            text("midi_thru").size(12).width(Length::Fixed(120.0)),
            checkbox(false)
                .label("enable")
                .on_toggle(|_| lumino_ui_core::message::null()),
        ]
        .spacing(8),
        row![
            text("sample_rate").size(12).width(Length::Fixed(120.0)),
            pick_list(sample_rates.to_vec(), Some(48000), |_| {
                lumino_ui_core::message::null()
            })
            .placeholder("rate")
            .padding(6),
            text("Hz").size(11),
        ]
        .spacing(8),
        row![
            text("buffer_size").size(12).width(Length::Fixed(120.0)),
            container(text("512").size(12))
                .padding(6)
                .style(move |_t: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(weak)),
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            button(text("refresh").size(11))
                .on_press(lumino_ui_core::message::null())
                .padding([4, 8]),
        ]
        .spacing(8),
    ]
    .spacing(10)
    .padding(12);

    container(content)
        .width(Length::Fill)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}
