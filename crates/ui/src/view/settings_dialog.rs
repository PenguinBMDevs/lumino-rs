use iced_core::Length;
use iced_widget::{button, column, container, row, space, text};

use crate::message::Message;
use crate::{settings, window};
use lumino_core::i18n::settings_translations;

/// 渲染设置对话框
pub fn view_settings_dialog<'a>(
    settings: &'a settings::SettingsPanel,
    window: &'a window::Window,
    system_fonts: &'a [lumino_core::font_scanner::FontInfo],
) -> crate::Element<'a> {
    let t = settings_translations(settings.language);
    let palette = window.theme.extended_palette();

    // 设置内容（复用现有的 settings::view）
    let settings_content = settings::view(settings, window, system_fonts);

    // 确认按钮
    let confirm_button = button(text(t.confirm).size(14))
        .on_press(Message::CloseSettingsDialog)
        .padding([8, 32])
        .width(Length::Fixed(100.0))
        .style(move |_theme: &iced_core::Theme, status| {
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
        });

    // 主内容
    let content = column![
        settings_content,
        space().height(8),
        row![space().width(Length::Fill), confirm_button].align_y(iced_core::Alignment::Center),
    ]
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
