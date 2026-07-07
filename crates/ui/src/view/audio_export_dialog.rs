use iced_core::Length;
use iced_widget::{
    button, checkbox, column, container, pick_list, row, scrollable, space, text, text_input,
};

use crate::message::{AudioExportAction, Message};
use crate::state::root_state::{
    AudioChannels, AudioExportDialogState, AudioFormat, Interpolation, ThreadingOption,
};

/// 渲染音频导出对话框
pub fn view_audio_export_dialog<'a>(
    state: &'a AudioExportDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    // 复选框样式（修复暗色主题文字反色）
    let checkbox_style = move |_theme: &iced_core::Theme,
                               _status: iced_widget::checkbox::Status| {
        iced_widget::checkbox::Style {
            background: iced_core::Background::Color(palette.background.weak.color),
            icon_color: palette.background.neutral.text,
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: palette.background.strong.color,
            },
            text_color: Some(palette.background.neutral.text),
        }
    };
    let label_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };

    // 输入框样式
    let input_style = move |_theme: &iced_core::Theme| container::Style {
        background: Some(palette.background.weak.color.into()),
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    };

    // 标题
    let title = text("音频导出")
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(label_style);

    // 工程信息区域
    let project_info = column![
        text("工程名称").size(14).style(label_style),
        space().height(4),
        container(
            text_input("工程名称", &state.project_name)
                .on_input(|v| Message::AudioExport(AudioExportAction::ProjectNameChanged(v)))
                .padding([6, 10])
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .style(input_style),
        space().height(12),
        text("MIDI 路径").size(14).style(label_style),
        space().height(4),
        row![
            container(
                text(&state.midi_path)
                    .size(12)
                    .style(move |_t: &iced_core::Theme| text::Style {
                        color: Some(palette.background.weak.text),
                    })
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .style(input_style),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::AudioExport(AudioExportAction::BrowseMidi))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(12),
        text("音色库 (SF2)").size(14).style(label_style),
        space().height(4),
        row![
            container(
                text(&state.soundfont_path)
                    .size(12)
                    .style(move |_t: &iced_core::Theme| text::Style {
                        color: Some(palette.background.weak.text),
                    })
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .style(input_style),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::AudioExport(AudioExportAction::BrowseSoundfont))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
    ]
    .width(Length::Fill);

    // 音频设置区域
    let audio_settings = column![
        text("音频设置")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(label_style),
        space().height(12),
        // 输出格式
        row![
            text("输出格式:").size(14).style(label_style).width(120),
            pick_list(
                [AudioFormat::WAV, AudioFormat::FLAC],
                Some(state.format),
                |v| Message::AudioExport(AudioExportAction::FormatChanged(v)),
            )
            .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        // 采样率
        row![
            text("采样率:").size(14).style(label_style).width(120),
            pick_list(
                [22050u32, 44100, 48000, 96000],
                Some(state.sample_rate),
                |v| Message::AudioExport(AudioExportAction::SampleRateChanged(v)),
            )
            .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        // 通道数
        row![
            text("通道数:").size(14).style(label_style).width(120),
            pick_list(
                [AudioChannels::Mono, AudioChannels::Stereo],
                Some(state.channels),
                |v| Message::AudioExport(AudioExportAction::ChannelsChanged(v)),
            )
            .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        // 层数限制
        row![
            text("层数限制:").size(14).style(label_style).width(120),
            text_input("32", &state.layers.to_string())
                .on_input(|v| Message::AudioExport(AudioExportAction::LayersChanged(v)))
                .padding([6, 10])
                .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        // 通道多线程
        row![
            text("通道多线程:").size(14).style(label_style).width(120),
            pick_list(
                [
                    ThreadingOption::None,
                    ThreadingOption::Auto,
                    ThreadingOption::Manual(2),
                    ThreadingOption::Manual(4),
                    ThreadingOption::Manual(8),
                ],
                Some(state.channel_threading),
                |v| Message::AudioExport(AudioExportAction::ChannelThreadingChanged(v)),
            )
            .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        // 按键多线程
        row![
            text("按键多线程:").size(14).style(label_style).width(120),
            pick_list(
                [
                    ThreadingOption::None,
                    ThreadingOption::Auto,
                    ThreadingOption::Manual(2),
                    ThreadingOption::Manual(4),
                    ThreadingOption::Manual(8),
                ],
                Some(state.key_threading),
                |v| Message::AudioExport(AudioExportAction::KeyThreadingChanged(v)),
            )
            .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        // 插值算法
        row![
            text("插值算法:").size(14).style(label_style).width(120),
            pick_list(
                [Interpolation::None, Interpolation::Linear],
                Some(state.interpolation),
                |v| Message::AudioExport(AudioExportAction::InterpolationChanged(v)),
            )
            .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(12),
        // 选项复选框
        checkbox(state.apply_limiter)
            .label("应用限制器 (防止削波)")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::ApplyLimiterChanged(v)))
            .style(checkbox_style),
        space().height(4),
        checkbox(state.disable_fade_out)
            .label("禁用淡出 (可能爆音)")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::DisableFadeOutChanged(v)))
            .style(checkbox_style),
        space().height(4),
        checkbox(state.linear_envelope)
            .label("线性包络")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::LinearEnvelopeChanged(v)))
            .style(checkbox_style),
    ]
    .width(Length::Fill);

    // 输出路径区域
    let output_path = column![
        text("输出路径")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(label_style),
        space().height(8),
        row![
            container(
                text_input("选择输出路径...", &state.output_path)
                    .on_input(|v| Message::AudioExport(AudioExportAction::OutputPathChanged(v)))
                    .padding([6, 10])
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .style(input_style),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::AudioExport(AudioExportAction::BrowseOutput))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
    ]
    .width(Length::Fill);

    // 进度区域（仅在导出时显示）
    let progress_area = if state.is_exporting {
        // 预构建统计 widget
        let note_on_count = state.note_on_processed;
        let note_off_count = state.note_off_processed;
        let note_on_widget: crate::Element<'_> = if note_on_count > 0 {
            row![
                text("NoteOn: ").size(12).style(label_style),
                text(note_on_count.to_string()).size(12)
                    .style(move |_t: &iced_core::Theme| text::Style {
                        color: Some(palette.primary.base.color),
                        ..Default::default()
                    }),
            ]
            .spacing(4)
            .into()
        } else {
            iced_widget::Space::new().into()
        };
        let note_off_widget: crate::Element<'_> = if note_off_count > 0 {
            row![
                text("NoteOff: ").size(12).style(label_style),
                text(note_off_count.to_string()).size(12)
                    .style(move |_t: &iced_core::Theme| text::Style {
                        color: Some(palette.secondary.base.color),
                        ..Default::default()
                    }),
            ]
            .spacing(4)
            .into()
        } else {
            iced_widget::Space::new().into()
        };
        Some(
            column![
                space().height(16),
                text(&state.status_message).size(14).style(label_style),
                space().height(8),
                // 进度条
                container(
                    column![
                        text(format!("{:.1}%", state.progress)).size(12).style(
                            move |_t: &iced_core::Theme| text::Style {
                                color: Some(palette.background.neutral.text),
                            }
                        ),
                    ]
                    .padding([2, 8]),
                )
                .width(Length::Fill)
                .height(24)
                .style(move |_t: &iced_core::Theme| container::Style {
                    background: Some(palette.background.weak.color.into()),
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        width: 1.0,
                        color: palette.background.strong.color,
                    },
                    ..Default::default()
                }),
                space().height(8),
                // 详细统计
                row![note_on_widget, space().width(16), note_off_widget,]
                .spacing(4)
                .align_y(iced_core::Alignment::Center),
            ]
            .width(Length::Fill),
        )
    } else {
        None
    };

    // 按钮区域
    let buttons = if state.is_exporting {
        // 导出中只显示取消按钮
        row![
            button(text("取消").size(14))
                .on_press(Message::AudioExport(AudioExportAction::Cancel))
                .padding([8, 32])
                .width(Length::Fixed(100.0))
                .style(move |_t: &iced_core::Theme, status| {
                    let bg = match status {
                        button::Status::Hovered => palette.background.strong.color,
                        _ => palette.background.weak.color,
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: palette.background.neutral.text,
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        snap: false,
                        shadow: Default::default(),
                    }
                }),
        ]
        .align_y(iced_core::Alignment::Center)
    } else {
        // 正常状态显示取消和导出按钮
        row![
            button(text("取消").size(14))
                .on_press(Message::AudioExport(AudioExportAction::CloseDialog))
                .padding([8, 32])
                .width(Length::Fixed(100.0))
                .style(move |_t: &iced_core::Theme, status| {
                    let bg = match status {
                        button::Status::Hovered => palette.background.strong.color,
                        _ => palette.background.weak.color,
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: palette.background.neutral.text,
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        snap: false,
                        shadow: Default::default(),
                    }
                }),
            space().width(12),
            button(text("导出").size(14))
                .on_press(Message::AudioExport(AudioExportAction::Confirm))
                .padding([8, 32])
                .width(Length::Fixed(100.0))
                .style(move |_t: &iced_core::Theme, status| {
                    let bg = match status {
                        button::Status::Hovered => palette.primary.strong.color,
                        _ => palette.primary.base.color,
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: iced_core::Color::WHITE,
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        snap: false,
                        shadow: Default::default(),
                    }
                }),
        ]
        .align_y(iced_core::Alignment::Center)
    };

    // 组装主内容
    let mut main_content = column![
        title,
        space().height(16),
        project_info,
        space().height(16),
        audio_settings,
        space().height(16),
        output_path,
    ];

    // 添加进度区域（如果有）
    if let Some(progress) = progress_area {
        main_content = main_content.push(progress);
    }

    main_content = main_content.push(space().height(24));
    main_content = main_content.push(buttons);

    let scrollable_content = scrollable(main_content)
        .width(Length::Fill)
        .height(Length::Fill);

    let dialog_content = container(scrollable_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_t: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
