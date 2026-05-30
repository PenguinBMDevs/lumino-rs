use iced_core::Length;
use iced_widget::{button, column, container, row, space, text};

use crate::message::Message;
use crate::{settings, window};

/// 渲染设置对话框
pub fn view_settings_dialog<'a>(
    settings: &'a settings::SettingsPanel,
    window: &'a window::Window,
    system_fonts: &'a [lumino_core::font_scanner::FontInfo],
) -> crate::Element<'a> {
    let palette = window.theme.extended_palette();

    // 标题栏样式
    let label_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };

    // 标题
    let title = text("设置")
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(label_style);

    // 关闭按钮
    let close_button = button(text("关闭").size(14))
        .on_press(Message::CloseSettingsDialog)
        .padding([8, 24])
        .width(Length::Fixed(80.0))
        .style(move |_theme: &iced_core::Theme, status| {
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
                shadow: Default::default(),
                snap: false,
            }
        });

    // 标题栏
    let title_bar = row![title, space().width(Length::Fill), close_button,]
        .align_y(iced_core::Alignment::Center)
        .width(Length::Fill);

    // 设置内容（复用现有的 settings::view）
    let settings_content = settings::view(settings, window, system_fonts);

    // 主内容
    let content = column![title_bar, space().height(8), settings_content,]
        .spacing(4)
        .width(Length::Fill)
        .height(Length::Fill);

    let dialog_content = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .style(move |_theme: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        });

    dialog_content.into()
}
