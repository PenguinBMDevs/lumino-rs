//! 文字参数面板（DRAW-029 · Parametric）
//!
//! 挂在绘制工具条「文字」条目上（再次点击已激活的文字工具打开），复用
//! `CurveToolGroup` 的锚定悬浮层，不进入钢琴卷帘的 `view_overlay`——避免与
//! 文字输入框抢焦点。面板只改 `TextToolState.params`，确认/取消后参数保留。

use iced_core::{Alignment, Background, Border, Color, Length};
use iced_widget::{button, column, container, pick_list, row, slider, text};
use lumino_core::{TextAlign, TextLayoutMode, TextParams, TextUnit};

use crate::message::EditorAction;
use crate::{Element, Message};

/// 面板内容宽度（与工具条 MENU_WIDTH 的内边距匹配）
const CONTENT_WIDTH: f32 = 220.0;
/// 左侧标签宽度
const LABEL_WIDTH: f32 = 62.0;
/// 滑杆宽度
const SLIDER_WIDTH: f32 = 116.0;
/// 数值显示宽度
const VALUE_WIDTH: f32 = 38.0;

/// 参数更新消息：整体回传，避免面板持有第二份权威
fn changed(params: &TextParams, f: impl FnOnce(&mut TextParams)) -> Message {
    let mut next = params.clone();
    f(&mut next);
    Message::EditorAction(EditorAction::TextParamsChanged(next))
}

/// 渲染参数面板
pub(crate) fn render_text_params(
    params: &TextParams,
    mode: TextLayoutMode,
    panel_background: Color,
    theme: &iced_core::Theme,
) -> Element<'static> {
    let border_color = theme.extended_palette().background.strong.color;
    let text_color = panel_text_color(panel_background);

    let mut fonts: Vec<String> = lumino_note_core::font_scanner::get_cached_fonts()
        .iter()
        .map(|font| font.name.clone())
        .collect();
    fonts.sort_unstable();
    fonts.dedup();
    let p_font = params.clone();
    let font_pick = pick_list(fonts, Some(params.font.clone()), move |font: String| {
        let mut next = p_font.clone();
        next.font = font;
        Message::EditorAction(EditorAction::TextParamsChanged(next))
    })
    .text_size(12)
    .padding([3, 6])
    .width(Length::Fixed(150.0));

    let font_row = row![label("Font", text_color), font_pick,]
        .spacing(6)
        .align_y(Alignment::Center);

    let size_row = slider_row(
        "Size",
        params.size,
        4.0..=96.0,
        params,
        text_color,
        |p, v| p.size = v,
    );
    let p_weight = params.clone();
    let weight_row = row![
        label("Weight", text_color),
        slider(100.0..=900.0, f32::from(params.weight), move |v| {
            let mut next = p_weight.clone();
            next.weight = v.round().clamp(100.0, 900.0) as u16;
            Message::EditorAction(EditorAction::TextParamsChanged(next))
        })
        .step(100.0_f32)
        .width(Length::Fixed(SLIDER_WIDTH)),
        text(format!("{}", params.weight))
            .size(11)
            .width(Length::Fixed(VALUE_WIDTH))
            .color(text_color),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let italic_row = row![
        label("Italic", text_color),
        small_button(
            "Italic",
            params.italic,
            changed(params, |p| p.italic = !p.italic),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let tracking_row = slider_row(
        "Tracking",
        params.tracking,
        -20.0..=20.0,
        params,
        text_color,
        |p, v| p.tracking = v,
    );
    let leading_row = slider_row(
        "Leading",
        params.leading,
        50.0..=200.0,
        params,
        text_color,
        |p, v| p.leading = v,
    );
    let threshold_row = slider_row(
        "Threshold",
        params.threshold,
        0.0..=100.0,
        params,
        text_color,
        |p, v| p.threshold = v,
    );
    let grow_row = slider_row(
        "Grow",
        params.grow,
        -10.0..=10.0,
        params,
        text_color,
        |p, v| p.grow = v,
    );

    let unit_row = row![
        label("Unit", text_color),
        small_button(
            "Font",
            params.unit == TextUnit::Font,
            changed(params, |p| p.unit = TextUnit::Font),
        ),
        small_button(
            "Rows",
            params.unit == TextUnit::Rows,
            changed(params, |p| p.unit = TextUnit::Rows),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let align_row = row![
        label("Align", text_color),
        small_button(
            "L",
            params.align == TextAlign::Left,
            changed(params, |p| p.align = TextAlign::Left),
        ),
        small_button(
            "C",
            params.align == TextAlign::Center,
            changed(params, |p| p.align = TextAlign::Center),
        ),
        small_button(
            "R",
            params.align == TextAlign::Right,
            changed(params, |p| p.align = TextAlign::Right),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let mode_label = match mode {
        TextLayoutMode::LegacyBox => "Parametric",
        TextLayoutMode::Parametric => "LegacyBox",
    };
    let mode_target = match mode {
        TextLayoutMode::LegacyBox => TextLayoutMode::Parametric,
        TextLayoutMode::Parametric => TextLayoutMode::LegacyBox,
    };
    let mode_row = row![
        label("Mode", text_color),
        text(match mode {
            TextLayoutMode::LegacyBox => "Legacy",
            TextLayoutMode::Parametric => "Parametric",
        })
        .size(12)
        .color(text_color),
        button(text(mode_label).size(11))
            .on_press(Message::EditorAction(EditorAction::TextLayoutModeChanged(
                mode_target
            )))
            .padding([2, 6]),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let content = column![
        font_row,
        size_row,
        weight_row,
        italic_row,
        unit_row,
        tracking_row,
        leading_row,
        align_row,
        threshold_row,
        grow_row,
        mode_row,
    ]
    .spacing(6)
    .width(Length::Fixed(CONTENT_WIDTH));

    container(content)
        .padding(12)
        .width(Length::Fixed(CONTENT_WIDTH + 24.0))
        .style(move |_theme: &iced_core::Theme| container::Style {
            background: Some(Background::Color(panel_background)),
            text_color: Some(text_color),
            border: Border {
                width: 1.0,
                color: border_color,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn slider_row(
    label_text: &'static str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    params: &TextParams,
    text_color: Color,
    apply: impl Fn(&mut TextParams, f32) + Copy + 'static,
) -> Element<'static> {
    let p = params.clone();
    let slider = slider(range, value, move |v| {
        let mut next = p.clone();
        apply(&mut next, v);
        Message::EditorAction(EditorAction::TextParamsChanged(next))
    })
    .step(0.1_f32)
    .width(Length::Fixed(SLIDER_WIDTH));
    row![
        label(label_text, text_color),
        slider,
        text(format!("{value:.1}"))
            .size(11)
            .width(Length::Fixed(VALUE_WIDTH))
            .color(text_color),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn label(value: &'static str, color: Color) -> Element<'static> {
    text(value)
        .size(12)
        .width(Length::Fixed(LABEL_WIDTH))
        .color(color)
        .into()
}

fn small_button(value: &'static str, selected: bool, message: Message) -> Element<'static> {
    let background = if selected {
        Some(Background::Color(Color::from_rgba(0.22, 0.42, 0.62, 0.9)))
    } else {
        None
    };
    button(text(value).size(11))
        .on_press(message)
        .padding([2, 6])
        .style(move |_theme: &iced_core::Theme, _status| button::Style {
            background,
            border: Border::default().rounded(4),
            ..Default::default()
        })
        .into()
}

/// 面板底色上的可读文字色：按背景亮度二选一，避免亮色主题黑字落在暗面板上。
fn panel_text_color(background: Color) -> Color {
    let luminance = 0.2126 * background.r + 0.7152 * background.g + 0.0722 * background.b;
    if luminance > 0.55 {
        Color::from_rgba(0.08, 0.08, 0.10, 1.0)
    } else {
        Color::from_rgba(0.95, 0.95, 0.95, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_text_params_builds_for_both_modes() {
        let theme = crate::Theme::Dark;
        let bg = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
        let params = TextParams {
            weight: 700,
            italic: true,
            ..TextParams::default()
        };
        for mode in [TextLayoutMode::LegacyBox, TextLayoutMode::Parametric] {
            let _ = render_text_params(&params, mode, bg, &theme);
        }
    }

    #[test]
    fn test_text_color_contrasts_with_panel_background() {
        let dark = panel_text_color(Color::from_rgba(0.1, 0.1, 0.1, 1.0));
        let light = panel_text_color(Color::from_rgba(0.9, 0.9, 0.9, 1.0));
        assert!(dark.r > 0.5, "暗底应使用浅色文字");
        assert!(light.r < 0.5, "亮底应使用深色文字");
    }
}
