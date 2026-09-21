//! 设置页面 - 音频设置 LGS (GPU) 选项渲染
//!
//! 由 `pages/audio.rs` 原样拆分，函数体逻辑零变更。

use iced_core::{Alignment, Length};
use iced_widget::{column, row, text, text_input};
use lumino_ui_core::{Message, Theme};

use super::super::super::components::constants::{
    SPACING_CONTENT, SPACING_ICON_LABEL, SPACING_MAIN, TEXT_SIZE_CONTENT,
};
use super::super::super::components::styles::{
    create_content_text_style, create_placeholder_text_style,
};
use crate::SettingsPanel;

/// 渲染 LGS (GPU) 选项
///
/// 与 XSynth 共用 `soundfont_path`；GPU 专属参数（渲染采样率/块大小/插值）
/// 通过内置 MIDI 输出组的统一控件暴露：缓冲区大小、每键最大同音数、响度过滤。
pub(super) fn render_lgs_options<'a>(
    settings: &SettingsPanel,
    t: &lumino_extras::i18n::SettingsTranslations,
) -> iced_widget::Column<'a, Message, Theme, lumino_ui_core::Renderer> {
    let mut col = column![];

    // 音色库选择（与 XSynth 共用 soundfont_path）
    col = col.push(
        row![
            text(t.soundfont)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
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

    // 缓冲区大小（GPU 块大小，2 的幂）：滑块以 2 的指数表示（64=2^6 … 8192=2^13）
    let block_index = ((settings.synth.lgs_block_size as f64).log2().round() as usize).clamp(6, 13);
    col = col.push(
        row![
            text(format!(
                "{}: {}",
                t.lgs_buffer, settings.synth.lgs_block_size
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(200.0),
            iced_widget::slider(6.0..=13.0, block_index as f32, |i| {
                Message::Settings(crate::Event::LgsBlockSizeChanged(1usize << i as u32))
            })
            .step(1.0_f32)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        text(t.lgs_buffer_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(iced_widget::space().height(20));

    // 每键最大同音数：0=不限制，1..128 拖拽 + 自定义输入
    let lgs_voices = settings.synth.lgs_max_voices_per_key;
    let display_voices = if lgs_voices == 0 {
        "不限制".to_string()
    } else {
        lgs_voices.to_string()
    };
    col = col.push(
        row![
            text(format!("{}: {}", t.max_voices, display_voices))
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(180.0),
            iced_widget::slider(0.0..=128.0, lgs_voices as f32, |v| {
                Message::Settings(crate::Event::LgsMaxVoicesChanged(v as usize))
            })
            .step(1.0_f32)
            .width(160.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        text(t.max_voices_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(iced_widget::space().height(20));

    // LGS (GPU) 专属响度过滤（与 XSynth 全局力度过滤相互独立；LGS 输出连接在 note_on 处实时丢弃过轻音符）
    col = col.push(
        row![
            text(format!(
                "{}: {}",
                t.velocity_filter, settings.synth.lgs_velocity_filter_threshold
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(180.0),
            iced_widget::slider(0..=127, settings.synth.lgs_velocity_filter_threshold, |v| {
                Message::Settings(crate::Event::LgsVelocityFilterChanged(v))
            },)
            .step(1)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        text(t.velocity_filter_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(iced_widget::space().height(20));

    // LGS (GPU) 提示
    col = col.push(
        text(t.lgs_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );

    col
}
