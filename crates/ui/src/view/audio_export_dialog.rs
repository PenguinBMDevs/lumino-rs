//! 音频导出对话框
//!
//! 重构说明：
//! - 所有 section 函数返回 `crate::Element`，通过 `.into()` 让 Renderer 类型推断为 `iced_wgpu`。
//! - 样式闭包每次都从 `widgets::dialog_*_style(palette)` 创建新闭包，
//!   避免借用局部变量导致 E0515。

use iced_widget::{
    button, checkbox, column, container, pick_list, progress_bar, row, scrollable, space, text,
    text_input,
};

use crate::message::{AudioExportAction, Message};
use crate::state::root_state::{
    AudioChannels, AudioExportDialogState, AudioFormat, Interpolation, ThreadingOption,
};

use super::widgets;

/// 渲染音频导出对话框
pub fn view_audio_export_dialog<'a>(
    state: &'a AudioExportDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let main_content = column![
        title_section(palette),
        space().height(16),
        project_info_section(state, palette),
        space().height(16),
        audio_settings_section(state, palette),
        space().height(16),
        event_filter_section(state, palette),
        space().height(16),
        output_path_section(state, palette),
        space().height(24),
        buttons_section(state, palette),
    ];

    let scrollable_content = scrollable(main_content)
        .width(iced_core::Length::Fill)
        .height(iced_core::Length::Fill);

    let dialog_content = container(scrollable_content)
        .width(iced_core::Length::Fill)
        .height(iced_core::Length::Fill)
        .padding(24)
        .style(move |_t: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}

/// 对话框大标题
fn title_section<'a>(palette: &'a iced_core::theme::palette::Extended) -> crate::Element<'a> {
    text("音频导出")
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(widgets::dialog_label_style(palette))
        .into()
}

/// 段落小标题
fn section_title<'a>(text_str: &'a str, palette: &'a iced_core::theme::palette::Extended) -> crate::Element<'a> {
    text(text_str)
        .size(16)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(widgets::dialog_label_style(palette))
        .into()
}

/// 工程信息区域（工程名、MIDI路径、音色库路径）
fn project_info_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("工程名称").size(14).style(widgets::dialog_label_style(palette)),
        space().height(4),
        container(
            text_input("工程名称", &state.project_name)
                .on_input(|v| Message::AudioExport(AudioExportAction::ProjectNameChanged(v)))
                .padding([6, 10])
                .width(iced_core::Length::Fill),
        )
        .width(iced_core::Length::Fill)
        .style(widgets::dialog_input_style(palette)),
        space().height(12),
        text("MIDI 路径").size(14).style(widgets::dialog_label_style(palette)),
        space().height(4),
        row![
            container(
                text(&state.midi_path)
                    .size(12)
                    .style(widgets::dialog_muted_text_style(palette))
                    .width(iced_core::Length::Fill),
            )
            .width(iced_core::Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::AudioExport(AudioExportAction::BrowseMidi))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(12),
        text("音色库 (SF2)").size(14).style(widgets::dialog_label_style(palette)),
        space().height(4),
        row![
            container(
                text(&state.soundfont_path)
                    .size(12)
                    .style(widgets::dialog_muted_text_style(palette))
                    .width(iced_core::Length::Fill),
            )
            .width(iced_core::Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::AudioExport(AudioExportAction::BrowseSoundfont))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 音频设置区域（输出格式、比特率、采样率、通道数、层数、线程、插值、复选框）
fn audio_settings_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        section_title("音频设置", palette),
        space().height(12),
        row![
            text("输出格式:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            pick_list(
                [AudioFormat::WAV, AudioFormat::FLAC, AudioFormat::MP3, AudioFormat::Ogg, AudioFormat::WavPack],
                Some(state.format),
                |v| Message::AudioExport(AudioExportAction::FormatChanged(v)),
            ),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        row![
            text("比特率 (kbps):").size(14).style(widgets::dialog_label_style(palette)).width(120),
            text_input("320", &state.audio_bitrate.to_string())
                .on_input(|v| Message::AudioExport(AudioExportAction::BitrateChanged(v)))
                .padding([6, 10])
                .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        row![
            text("采样率:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            pick_list(
                [22050u32, 44100, 48000, 96000],
                Some(state.sample_rate),
                |v| Message::AudioExport(AudioExportAction::SampleRateChanged(v)),
            ),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        row![
            text("通道数:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            pick_list(
                [AudioChannels::Mono, AudioChannels::Stereo],
                Some(state.channels),
                |v| Message::AudioExport(AudioExportAction::ChannelsChanged(v)),
            ),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        row![
            text("层数限制:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            text_input("32", &state.layers.to_string())
                .on_input(|v| Message::AudioExport(AudioExportAction::LayersChanged(v)))
                .padding([6, 10])
                .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        row![
            text("通道多线程:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            pick_list(
                [ThreadingOption::None, ThreadingOption::Auto, ThreadingOption::Manual(2), ThreadingOption::Manual(4), ThreadingOption::Manual(8)],
                Some(state.channel_threading),
                |v| Message::AudioExport(AudioExportAction::ChannelThreadingChanged(v)),
            ),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        row![
            text("按键多线程:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            pick_list(
                [ThreadingOption::None, ThreadingOption::Auto, ThreadingOption::Manual(2), ThreadingOption::Manual(4), ThreadingOption::Manual(8)],
                Some(state.key_threading),
                |v| Message::AudioExport(AudioExportAction::KeyThreadingChanged(v)),
            ),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        row![
            text("插值算法:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            pick_list(
                [Interpolation::None, Interpolation::Linear],
                Some(state.interpolation),
                |v| Message::AudioExport(AudioExportAction::InterpolationChanged(v)),
            ),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        space().height(12),
        checkbox(state.apply_limiter)
            .label("应用限制器 (防止削波)")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::ApplyLimiterChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        checkbox(state.disable_fade_out)
            .label("禁用淡出 (可能爆音)")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::DisableFadeOutChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        checkbox(state.linear_envelope)
            .label("线性包络")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::LinearEnvelopeChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 事件过滤区域（忽略音色变化、力度过滤、键位过滤、结束延迟）
fn event_filter_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    // 将临时 String 绑定到局部变量，避免 E0515
    let velocity_low = state.velocity_low.to_string();
    let velocity_high = state.velocity_high.to_string();
    let key_low = state.key_low.to_string();
    let key_high = state.key_high.to_string();
    let note_force_end_delay = state.note_force_end_delay.to_string();

    column![
        section_title("事件过滤", palette),
        space().height(8),
        checkbox(state.ignore_program_changes)
            .label("忽略音色变化事件")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::IgnoreProgramChangesChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(8),
        text("音符力度过滤").size(14).style(widgets::dialog_label_style(palette)),
        space().height(4),
        checkbox(state.filter_velocity)
            .label("启用力度过滤")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::FilterVelocityChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        row![
            text("力度范围:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            text_input("0", &velocity_low)
                .on_input(|v| Message::AudioExport(AudioExportAction::VelocityLowChanged(v)))
                .padding([6, 10])
                .width(80),
            text(" ~ ").size(14).style(widgets::dialog_label_style(palette)),
            text_input("127", &velocity_high)
                .on_input(|v| Message::AudioExport(AudioExportAction::VelocityHighChanged(v)))
                .padding([6, 10])
                .width(80),
        ]
        .spacing(4)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        text("音符键位过滤").size(14).style(widgets::dialog_label_style(palette)),
        space().height(4),
        checkbox(state.filter_key)
            .label("启用键位过滤")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::FilterKeyChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        row![
            text("键位范围:").size(14).style(widgets::dialog_label_style(palette)).width(120),
            text_input("0", &key_low)
                .on_input(|v| Message::AudioExport(AudioExportAction::KeyLowChanged(v)))
                .padding([6, 10])
                .width(80),
            text(" ~ ").size(14).style(widgets::dialog_label_style(palette)),
            text_input("127", &key_high)
                .on_input(|v| Message::AudioExport(AudioExportAction::KeyHighChanged(v)))
                .padding([6, 10])
                .width(80),
        ]
        .spacing(4)
        .align_y(iced_core::Alignment::Center),
        space().height(8),
        row![
            text("音符结束延迟 (ms):")
                .size(14)
                .style(widgets::dialog_label_style(palette))
                .width(120),
            text_input("0", &note_force_end_delay)
                .on_input(|v| Message::AudioExport(AudioExportAction::NoteForceEndDelayChanged(v)))
                .padding([6, 10])
                .width(200),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 输出路径区域
fn output_path_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        section_title("输出路径", palette),
        space().height(8),
        row![
            container(
                text_input("选择输出路径...", &state.output_path)
                    .on_input(|v| Message::AudioExport(AudioExportAction::OutputPathChanged(v)))
                    .padding([6, 10])
                    .width(iced_core::Length::Fill),
            )
            .width(iced_core::Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::AudioExport(AudioExportAction::BrowseOutput))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 按钮区域
fn buttons_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    if state.is_rendering {
        render_progress(state, palette)
    } else {
        action_buttons(palette)
    }
}

/// 渲染中：进度条
fn render_progress<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
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
    .width(iced_core::Length::Fill)
    .into()
}

/// 非渲染中：关闭/导出按钮
fn action_buttons(palette: &iced_core::theme::palette::Extended) -> crate::Element<'static> {
    row![
        button(text("关闭").size(14))
            .on_press(Message::AudioExport(AudioExportAction::ClosePanel))
            .padding([8, 32])
            .width(iced_core::Length::Fixed(100.0))
            .style(widgets::dialog_button_style(
                palette.background.strong.color,
                palette.background.weak.color,
                palette.background.neutral.text,
            )),
        space().width(12),
        button(text("导出").size(14))
            .on_press(Message::AudioExport(AudioExportAction::Confirm))
            .padding([8, 32])
            .width(iced_core::Length::Fixed(100.0))
            .style(widgets::dialog_button_style(
                palette.primary.strong.color,
                palette.primary.base.color,
                iced_core::Color::WHITE,
            )),
    ]
    .align_y(iced_core::Alignment::Center)
    .into()
}
