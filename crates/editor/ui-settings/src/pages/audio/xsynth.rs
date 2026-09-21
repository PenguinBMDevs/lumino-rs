//! 设置页面 - 音频设置 XSynth 选项渲染
//!
//! 由 `pages/audio.rs` 原样拆分，函数体逻辑零变更。

use iced_core::{Alignment, Length};
use iced_widget::{column, row, text, text_input};
use lumino_ui_core::{Message, Theme};

use super::super::super::components::constants::{
    SPACING_CONTENT, SPACING_ICON_LABEL, SPACING_MAIN, TEXT_SIZE_CONTENT,
};
use super::super::super::components::styles::create_content_text_style;
use super::super::super::components::{
    with_setting_tooltip_inline, with_setting_tooltip_inline_action,
};
use crate::SettingsPanel;

/// 渲染 XSynth 选项
pub(super) fn render_xsynth_options<'a>(
    settings: &SettingsPanel,
    t: &lumino_extras::i18n::SettingsTranslations,
) -> iced_widget::Column<'a, Message, Theme, lumino_ui_core::Renderer> {
    let mut col = column![];

    // 音色库选择（XSynth 引擎说明挂在「音色库」标签文字上，控件本身不触发提示）
    col = col.push(
        row![
            with_setting_tooltip_inline(
                text(t.soundfont)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.xsynth_hint,
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input(t.soundfont_placeholder, &settings.synth.soundfont_path)
                .width(Length::Fill)
                .on_input(|s| Message::Settings(crate::Event::SoundfontPathChanged(s))),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        iced_widget::button(t.browse).on_press(Message::Settings(crate::Event::BrowseSoundfont)),
    );
    col = col.push(iced_widget::space().height(20));

    // 缓冲区大小（Realtime 引擎使用毫秒粒度）
    col = col.push(
        row![
            text(format!(
                "{}: {:.1} ms",
                t.buffer_latency, settings.synth.xsynth_buffer_ms
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(160.0),
            iced_widget::slider(5.0..=100.0, settings.synth.xsynth_buffer_ms, |ms| {
                Message::Settings(crate::Event::XSynthBufferChanged(ms))
            })
            .step(1.0_f32)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 每键最大同音数：0=不限制，1..64 拖拽 + 1..128 自定义输入
    let slider_val = match settings.synth.xsynth_max_voices_per_key {
        None => 0.0,
        Some(v) => (v as f32).clamp(0.0, 64.0),
    };
    let display_val = match settings.synth.xsynth_max_voices_per_key {
        None => "不限制".to_string(),
        Some(v) => v.to_string(),
    };
    col = col.push(
        row![
            with_setting_tooltip_inline(
                text(format!("{}: {}", t.max_voices, display_val))
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style())
                    .width(180.0),
                t.max_voices_hint,
            ),
            iced_widget::slider(0.0..=64.0, slider_val, |v| {
                let opt = if v < 0.5 { None } else { Some(v as usize) };
                Message::Settings(crate::Event::XSynthMaxVoicesChanged(opt))
            })
            .step(1.0_f32)
            .width(160.0),
            text_input("0=不限制 1-128", &display_val)
                .width(80.0)
                .on_input(|s| { Message::Settings(crate::Event::XSynthMaxVoicesCustomInput(s)) }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(20));

    // 全局最大复音数（硬上限/量程）：0=自动（引擎默认 10000）
    let global_limit_val = settings.synth.xsynth_global_voice_limit.unwrap_or(0);
    let global_display = match settings.synth.xsynth_global_voice_limit {
        None => format!("{} (10000)", t.global_voice_limit_auto),
        Some(v) => v.to_string(),
    };
    col = col.push(
        row![
            with_setting_tooltip_inline(
                text(format!("{}: {}", t.global_voice_limit, global_display))
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style())
                    .width(200.0),
                t.global_voice_limit_hint,
            ),
            iced_widget::slider(0.0..=20000.0, global_limit_val as f64, |v| {
                Message::Settings(crate::Event::XSynthGlobalVoiceLimitChanged(v as usize))
            })
            .step(500.0_f32)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 复音软目标比例（默认 1-1/e≈0.632）
    col = col.push(
        row![
            with_setting_tooltip_inline(
                text(format!(
                    "{}: {:.3}",
                    t.voice_target_ratio, settings.synth.xsynth_voice_target_ratio
                ))
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(200.0),
                t.voice_target_ratio_hint,
            ),
            iced_widget::slider(0.50..=0.90, settings.synth.xsynth_voice_target_ratio, |r| {
                Message::Settings(crate::Event::XSynthVoiceTargetRatioChanged(r))
            })
            .step(0.001_f32)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 过载保命闸（软 NPS 闸，默认关闭）
    // 说明挂在独立标签文字上（提示不覆盖对钩框）；点击文字等价于切换对钩
    col = col.push(
        row![
            iced_widget::Checkbox::new(settings.synth.xsynth_soft_nps_gate)
                .on_toggle(|on| Message::Settings(crate::Event::XSynthSoftNpsGateChanged(on))),
            with_setting_tooltip_inline_action(
                text(t.soft_nps_gate)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.soft_nps_gate_hint,
                Message::Settings(crate::Event::XSynthSoftNpsGateChanged(
                    !settings.synth.xsynth_soft_nps_gate,
                )),
            ),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(20));

    // 力度过滤
    col = col.push(
        row![
            with_setting_tooltip_inline(
                text(format!(
                    "{}: {}",
                    t.velocity_filter, settings.midi.velocity_filter_threshold
                ))
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(180.0),
                t.velocity_filter_hint,
            ),
            iced_widget::slider(0..=127, settings.midi.velocity_filter_threshold, |v| {
                Message::Settings(crate::Event::VelocityFilterThresholdChanged(v.to_string()))
            })
            .step(1)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(20));

    col
}
