//! 对话框共享 UI widget 样式与构建器
//!
//! 消除 audio_export_dialog、video_export_dialog 等视图间重复的样式闭包定义。
//! 所有对话框使用同一组样式函数，确保主题一致性并减少复制粘贴。

use iced_core::theme::palette::Extended;
use iced_widget::{container, text};

/// 对话框标签文本样式
pub fn dialog_label_style<'a>(
    palette: &'a Extended,
) -> impl Fn(&iced_core::Theme) -> text::Style + 'a {
    move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    }
}

/// 对话框输入框容器样式
pub fn dialog_input_style<'a>(
    palette: &'a Extended,
) -> impl Fn(&iced_core::Theme) -> container::Style + 'a {
    move |_theme: &iced_core::Theme| container::Style {
        background: Some(palette.background.weak.color.into()),
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    }
}

/// 对话框单选框样式（暗色主题文字反色已修复）
pub fn dialog_checkbox_style<'a>(
    palette: &'a Extended,
) -> impl Fn(&iced_core::Theme, iced_widget::checkbox::Status) -> iced_widget::checkbox::Style + 'a {
    move |_theme: &iced_core::Theme, _status: iced_widget::checkbox::Status| {
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
    }
}

/// 对话框次要文本样式（灰色提示文字）
pub fn dialog_muted_text_style<'a>(
    palette: &'a Extended,
) -> impl Fn(&iced_core::Theme) -> text::Style + 'a {
    move |_t: &iced_core::Theme| text::Style {
        color: Some(palette.background.weak.text),
    }
}

/// 对话框按钮通用背景样式
pub fn dialog_button_style(
    bg_hover: iced_core::Color,
    bg_normal: iced_core::Color,
    text_color: iced_core::Color,
) -> impl Fn(&iced_core::Theme, iced_widget::button::Status) -> iced_widget::button::Style {
    move |_t: &iced_core::Theme, status| {
        let bg = match status {
            iced_widget::button::Status::Hovered => bg_hover,
            _ => bg_normal,
        };
        iced_widget::button::Style {
            background: Some(bg.into()),
            text_color,
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 0.0,
                color: iced_core::Color::TRANSPARENT,
            },
            snap: false,
            shadow: Default::default(),
        }
    }
}
