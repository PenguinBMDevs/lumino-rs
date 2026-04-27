//! 设置页面 - 音频设置

use crate::{Element, Message, Theme};
use iced_core::{Alignment, Length};
use iced_widget::{column, pick_list, row, text, text_input};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::settings::SettingsPanel;
use lumino_core::storage::config::SynthBackend;

/// 渲染音频设置页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    let synth_options = [
        SynthBackend::XSynth,
        SynthBackend::Kdmapi,
        SynthBackend::System,
    ];

    let mut col = column![
        text("音频")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 合成器后端选择
        row![
            text("合成器:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(synth_options, Some(settings.synth_backend), |backend| {
                Message::Settings(crate::settings::Event::SynthBackendChanged(backend))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
    ];

    // 只在 XSynth 模式下显示音色库选择
    if settings.synth_backend == SynthBackend::XSynth {
        col = col.push(render_xsynth_options(settings));
    } else if settings.synth_backend == SynthBackend::Kdmapi {
        col = col.push(
            text("KDMAPI 模式使用系统驱动，无需音色库")
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        );
    } else if settings.synth_backend == SynthBackend::System {
        col = col.push(
            text("System 模式使用系统默认的WinMM MIDI输出，无需音色库")
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        );
    }

    col.spacing(SPACING_CONTENT).padding(PADDING_CONTENT).into()
}

/// 渲染 XSynt 选项
fn render_xsynth_options<'a>(
    settings: &SettingsPanel,
) -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    let mut col = column![];

    // 音色库选择
    col = col.push(
        row![
            text("音色库:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("选择音色库文件 (SFZ/SF2)...", &settings.soundfont_path)
                .width(Length::Fill)
                .on_input(|s| Message::Settings(crate::settings::Event::SoundfontPathChanged(s))),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        iced_widget::button("浏览...")
            .on_press(Message::Settings(crate::settings::Event::BrowseSoundfont)),
    );
    col = col.push(iced_widget::space().height(20));

    // 采样率
    let sample_rates = [44100u32, 48000, 88200, 96000];
    col = col.push(
        row![
            text("采样率:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(sample_rates, Some(settings.xsynth_sample_rate), |sr| {
                Message::Settings(crate::settings::Event::XSynthSampleRateChanged(sr))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 缓冲区大小
    col = col.push(
        row![
            text(format!(
                "缓冲区 (延迟): {:.1} ms",
                settings.xsynth_buffer_ms
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(160.0),
            iced_widget::slider(5.0..=100.0, settings.xsynth_buffer_ms, |ms| {
                Message::Settings(crate::settings::Event::XSynthBufferChanged(ms))
            })
            .step(1.0)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 多线程选项
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ThreadOption(i32, &'static str);
    impl std::fmt::Display for ThreadOption {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.1)
        }
    }
    let thread_options = [
        ThreadOption(-1, "关闭"),
        ThreadOption(0, "自动"),
        ThreadOption(1, "1 线程"),
        ThreadOption(2, "2 线程"),
        ThreadOption(4, "4 线程"),
        ThreadOption(8, "8 线程"),
    ];
    let current_thread_option = thread_options
        .iter()
        .find(|o| o.0 == settings.xsynth_threads)
        .copied()
        .or(Some(thread_options[1]));

    col = col.push(
        row![
            text("多线程渲染:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(thread_options, current_thread_option, |opt| {
                Message::Settings(crate::settings::Event::XSynthThreadsChanged(opt.0))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 音符释放淡出
    col = col.push(
        iced_widget::Checkbox::new(settings.xsynth_fade_out)
            .label("释放音符时平滑淡出 (防止爆音)")
            .on_toggle(|f| Message::Settings(crate::settings::Event::XSynthFadeOutChanged(f))),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 每键最大同音数
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct VoiceOption(Option<usize>, &'static str);
    impl std::fmt::Display for VoiceOption {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.1)
        }
    }
    let voice_options = [
        VoiceOption(Some(1), "1 (极保守)"),
        VoiceOption(Some(2), "2"),
        VoiceOption(Some(4), "4 (默认)"),
        VoiceOption(Some(8), "8"),
        VoiceOption(Some(16), "16 (推荐)"),
        VoiceOption(Some(32), "32"),
        VoiceOption(Some(64), "64 (密集)"),
        VoiceOption(None, "不限制"),
    ];
    let current_voice = voice_options
        .iter()
        .find(|o| o.0 == settings.xsynth_max_voices_per_key)
        .copied()
        .or(Some(voice_options[3]));

    col = col.push(
        row![
            text("每键最大同音数:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(voice_options, current_voice, |opt| {
                Message::Settings(crate::settings::Event::XSynthMaxVoicesChanged(opt.0))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        text("同键快速重复/密集和弦时，提高此值减少 voice stealing 导致的断音")
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(iced_widget::space().height(20));

    // 力度过滤
    col = col.push(
        row![
            text(format!(
                "力度过滤阈值: {}",
                settings.velocity_filter_threshold
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(180.0),
            iced_widget::slider(0..=127, settings.velocity_filter_threshold, |v| {
                Message::Settings(crate::settings::Event::VelocityFilterThresholdChanged(
                    v.to_string(),
                ))
            })
            .step(1)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        text("力度小于等于阈值的音符将不播放（0=关闭过滤）")
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(iced_widget::space().height(20));

    // 帮助文本
    col = col.push(
        text("XSynth: 内置高性能合成器，支持SFZ/SF2格式音色库")
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(
        text("KDMAPI: 使用系统KDMAPI驱动，需要安装OmniMIDI")
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(
        text("System: 使用系统默认的WinMM MIDI输出")
            .size(12.0)
            .style(create_placeholder_text_style()),
    );

    col
}
