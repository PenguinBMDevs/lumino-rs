use iced_core::Length;
use iced_widget::{
    button, checkbox, column, container, pick_list, progress_bar, row, scrollable, space, text,
    text_input,
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
        space().height(4),
        checkbox(state.use_gpu)
            .label("GPU 加速渲染 (推荐)")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::UseGpuChanged(v)))
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

    // 按钮区域 / 渲染进度区域
    let buttons: crate::Element<'a> = if state.is_rendering {
        // 渲染中：显示内嵌进度条
        let status_text = if state.render_completed {
            text("导出完成")
                .size(14)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(palette.success.strong.color),
                })
        } else if let Some(ref err) = state.render_error {
            text(format!("导出失败: {err}"))
                .size(14)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(palette.danger.strong.color),
                })
        } else {
            text(&state.render_message)
                .size(14)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(palette.background.neutral.text),
                })
        };

        column![
            status_text,
            space().height(8),
            progress_bar(0.0..=1.0, state.render_progress as f32),
            space().height(4),
            text(format!("{:.1}%", state.render_progress * 100.0))
                .size(12)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(palette.background.strong.text),
                }),
        ]
        .width(Length::Fill)
        .into()
    } else {
        row![
            button(text("关闭").size(14))
                .on_press(Message::AudioExport(AudioExportAction::ClosePanel))
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
        .into()
    };

    // 组装主内容
    let main_content = column![
        title,
        space().height(16),
        project_info,
        space().height(16),
        audio_settings,
        space().height(16),
        output_path,
        space().height(24),
        buttons,
    ];

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
