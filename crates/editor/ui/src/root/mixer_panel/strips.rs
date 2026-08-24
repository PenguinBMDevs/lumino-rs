//! 混音台通道条 / 主音量条 / 电平表渲染辅助函数。
//!
//! 抽离自 `mixer_panel.rs` 以控制单文件长度；常量（父模块私有）经 `use super::...`
//! 引入，类型与 iced 组件导入与父模块保持一致。

use super::{FADER_HEIGHT, METER_WIDTH, STRIP_WIDTH};
use crate::root::Root;
use crate::sidebar::{MIXER_MAX_VOLUME, gain_to_volume, volume_to_gain};
use crate::{Element, Theme, sidebar::Track};
use iced_core::{Alignment, Background, Color, Length};
use iced_widget::{Space, button, column, container, row, text, vertical_slider};
use lumino_ui_core::sidebar_event::Event;

/// 混音台文本主题样式：跟随主题基底文字色（暗色系白字 / 亮色系黑字），
/// 避免硬编码黑/白导致在亮/暗主题下不可读。
pub(crate) fn mixer_text_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.extended_palette().background.base.text),
    }
}

/// 音量刻度在标尺中的纵向偏移（自顶向下，单位逻辑像素）。
///
/// 线性映射：值 127 对应顶端（y=0），值 0 对应底端（y=FADER_HEIGHT）；
/// 与纵向推子拇指的实际行程一致，保证刻度间距与推子行程成正比（不会出现
/// 前半段 0-100、后半段 101-127 这类间距不一致）。
pub(crate) fn ruler_tick_offset(value: u8) -> f32 {
    (1.0 - value as f32 / 127.0) * FADER_HEIGHT
}

/// 单条音轨的通道条：名称 + M/S + 电平表 + 增益推子（纵向）+ 声像（横向）。
pub(crate) fn build_strip(root: &Root, track: &Track) -> Element<'static> {
    // 提取为拥有值，避免闭包捕获 `&Track` 引用导致生命周期受限。
    let id = track.id;
    let is_muted = track.is_muted;
    let is_soloed = track.is_soloed;
    let display_label = track.display_label.clone();
    let strip = root.sidebar.mixer_strip(id);
    let name = text(display_label).size(11).style(mixer_text_style);
    let mute_btn = button(text("M").size(11))
        .padding(2)
        .style(move |theme: &Theme, _status| channel_button_style(theme, is_muted))
        .on_press(Event::track_mute_toggled(id));
    let solo_btn = button(text("S").size(11))
        .padding(2)
        .style(move |theme: &Theme, _status| channel_button_style(theme, is_soloed))
        .on_press(Event::track_solo_toggled(id));

    // 音量推子：0..=127（MIDI 风格，100 为默认，127 对应 0 dB），纵向推子。
    let volume = gain_to_volume(strip.gain);
    let gain = vertical_slider(
        0.0_f32..=(MIXER_MAX_VOLUME as f32),
        volume as f32,
        move |v| Event::track_gain_changed(id, volume_to_gain(v as u8)),
    )
    .height(Length::Fixed(FADER_HEIGHT))
    .step(1.0_f32);

    // 音量标尺：纵向刻度按线性映射（0..127 对应 0..FADER_HEIGHT，自下而上）等比例定位，
    // 保证刻度间距与推子行程一致（值间距 = 像素间距），避免"前半段 0-100、后半段
    // 101-127"这类间距不一致；刻度位置与推子拇指实际位置对齐。
    let ruler_ticks: [(u8, f32); 4] = [
        (127, ruler_tick_offset(127)),
        (100, ruler_tick_offset(100)),
        (50, ruler_tick_offset(50)),
        (0, ruler_tick_offset(0)),
    ];
    let mut ruler_items: Vec<Element<'static>> = Vec::with_capacity(ruler_ticks.len() * 2);
    let mut prev_y = 0.0f32;
    for (val, y) in ruler_ticks {
        let gap = (y - prev_y).max(0.0);
        if gap > 0.0 {
            ruler_items.push(Space::new().height(Length::Fixed(gap)).into());
        }
        ruler_items.push(text(val.to_string()).size(9).style(mixer_text_style).into());
        prev_y = y;
    }
    let ruler = column(ruler_items)
        .spacing(0)
        .height(Length::Fixed(FADER_HEIGHT))
        .align_x(Alignment::End);

    // 实时响度峰值：每帧从播放引擎帧快照读取该通道当前演奏响度（0 表示无声）。
    let level = root
        .playback
        .manager
        .as_ref()
        .and_then(|m| m.last_frame())
        .map(|f| {
            f.channel_levels
                .get(track.channel as usize)
                .copied()
                .unwrap_or(0.0)
        })
        .unwrap_or(0.0);
    let meter = build_level_meter(level);
    let vol_readout = text(format!("音量 {volume}"))
        .size(10)
        .style(mixer_text_style);

    column![
        name,
        row![mute_btn, solo_btn].spacing(4),
        row![ruler, meter, gain]
            .spacing(4)
            .align_y(Alignment::Center),
        vol_readout,
    ]
    .spacing(6)
    .width(Length::Fixed(STRIP_WIDTH))
    .align_x(Alignment::Center)
    .into()
}

/// 全局音量控制器（混音台首项）：主音量推子 + 电平表 + 读数，无 M/S 与声像。
pub(crate) fn build_master_strip(root: &Root) -> Element<'static> {
    let master_volume = root.mixer_panel.master_volume;
    let name = text("主音量").size(11).style(mixer_text_style);

    // 主音量推子：0..=127，纵向，变化即时同步到播放引擎（全局缩放所有通道）。
    let gain = vertical_slider(
        0.0_f32..=(MIXER_MAX_VOLUME as f32),
        master_volume as f32,
        move |v| Event::mixer_panel_master_volume_changed(v as u8),
    )
    .height(Length::Fixed(FADER_HEIGHT))
    .step(1.0_f32);

    // 实时主输出响度峰值：每帧从播放引擎帧快照读取（0 表示无声）。
    let master_level = root
        .playback
        .manager
        .as_ref()
        .and_then(|m| m.last_frame())
        .map(|f| f.master_level)
        .unwrap_or(0.0);
    let meter = build_level_meter(master_level);
    let vol_readout = text(format!("音量 {master_volume}"))
        .size(10)
        .style(mixer_text_style);

    column![
        name,
        row![meter, gain].spacing(4).align_y(Alignment::Center),
        vol_readout,
    ]
    .spacing(6)
    .width(Length::Fixed(STRIP_WIDTH))
    .align_x(Alignment::Center)
    .into()
}

/// 电平呈现条（纵向）：以实时响度峰值（振幅 0..≈1）为填充高度，绿→黄→红分档着色。
///
/// 峰值来自播放引擎每帧推送的 `PlaybackFrame`（XSynth 在合成管线测量各通道/主输出
/// 实际样本振幅），即"真实演奏响度"，随音符的发声/力度/增益动态变化。
pub(crate) fn build_level_meter(level: f32) -> Element<'static> {
    let ratio = level.clamp(0.0, 1.0);
    let fill_h = (FADER_HEIGHT * ratio).clamp(2.0, FADER_HEIGHT);
    let color = meter_color(level);
    container(
        container(Space::new().height(Length::Fixed(fill_h)))
            .width(Length::Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(color)),
                ..Default::default()
            }),
    )
    .width(Length::Fixed(METER_WIDTH))
    .height(Length::Fixed(FADER_HEIGHT))
    .align_y(Alignment::End)
    .style(meter_bg_style)
    .into()
}

/// 电平条底色（轨道槽）
pub(crate) fn meter_bg_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(p.background.strong.color)),
        ..Default::default()
    }
}

/// 电平分档着色（按振幅）：≤0.5 绿（安全）；≤0.8 黄（接近上限）；否则橙红（削波附近）。
pub(crate) fn meter_color(level: f32) -> Color {
    if level <= 0.5 {
        Color::from_rgb(0.18, 0.78, 0.36)
    } else if level <= 0.8 {
        Color::from_rgb(0.95, 0.82, 0.18)
    } else {
        Color::from_rgb(0.92, 0.32, 0.24)
    }
}

/// 通道条 M/S 按钮样式（激活时高亮）
pub(crate) fn channel_button_style(theme: &Theme, active: bool) -> button::Style {
    let p = theme.extended_palette();
    button::Style {
        background: Some(Background::Color(if active {
            p.primary.strong.color
        } else {
            p.background.weak.color
        })),
        text_color: p.background.base.text,
        ..Default::default()
    }
}

/// 透明背景按钮（入口按钮 / 标题栏按钮）
pub(crate) fn transparent_button(theme: &Theme, _status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    button::Style {
        text_color: p.background.base.text,
        ..Default::default()
    }
    .with_background(Color::TRANSPARENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meter_color_zones() {
        // 安全区（≤0.5）同色、接近上限区（≤0.8）同色、削波附近（>0.8）同色，
        // 且安全区与削波附近不同色。
        assert_eq!(meter_color(0.0), meter_color(0.5));
        assert_eq!(meter_color(0.6), meter_color(0.8));
        assert_eq!(meter_color(0.9), meter_color(1.2));
        assert!(meter_color(0.2) != meter_color(0.9));
    }

    #[test]
    fn test_ruler_tick_offset_proportional() {
        // 127 在顶端（y=0），0 在底端（y=FADER_HEIGHT）。
        assert_eq!(ruler_tick_offset(127), 0.0);
        assert_eq!(ruler_tick_offset(0), FADER_HEIGHT);
        // 线性：100 与 50 的偏移差应等于 50 与 0 的偏移差（等间距），
        // 即刻度间距与推子行程成正比，无 0-100 / 100-127 的间距突变。
        let gap_high = ruler_tick_offset(50) - ruler_tick_offset(100);
        let gap_low = ruler_tick_offset(0) - ruler_tick_offset(50);
        assert!((gap_high - gap_low).abs() < 1e-3);
    }
}
