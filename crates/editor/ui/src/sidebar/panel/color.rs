//! 音轨选项卡颜色辅助函数

use iced_core::Color;

use crate::Theme;

/// 根据音轨颜色和状态计算选项卡背景色
pub fn track_button_background(
    color: Option<Color>,
    is_selected: bool,
    status: iced_widget::button::Status,
    theme: &Theme,
) -> Color {
    let palette = theme.extended_palette();
    match color {
        Some(c) => {
            if is_selected {
                lumino_ui_core::color::blend_color(c, palette.background.strong.color, 0.35)
            } else if status == iced_widget::button::Status::Hovered {
                lumino_ui_core::color::blend_color(c, palette.background.weak.color, 0.25)
            } else {
                c
            }
        }
        None => {
            if is_selected {
                palette.background.strong.color
            } else if status == iced_widget::button::Status::Hovered {
                palette.background.weak.color
            } else {
                palette.background.base.color
            }
        }
    }
}

/// 计算在指定背景上可读的文本颜色
pub fn track_text_color(color: Option<Color>, theme: &Theme) -> Color {
    match color {
        Some(c) => lumino_ui_core::color::contrast_text_color(c),
        None => theme.extended_palette().background.base.text,
    }
}
