use iced_core::{Alignment, Color, Length};
use iced_widget::{
    Space, Stack, button, column, container, mouse_area, row, space, text, text_input,
};

use crate::message::{Message, SettingsDialogAction};
use crate::{settings, window};
use lumino_extras::i18n::settings_translations;

/// 渲染设置对话框
///
/// 自定义 BPM 上限输入面板（`tempo_custom_open` 时）作为悬浮层渲染在
/// 设置窗口最顶层：底层为完整设置内容（保持可交互的布局不变），上层为
/// 半透明遮罩 + 居中输入卡片。悬浮层不参与 scrollable/column 布局，
/// 避免内嵌导致的布局塌陷。
pub fn view_settings_dialog<'a>(
    settings: &'a settings::SettingsPanel,
    window: &'a window::Window,
    system_fonts: &'a [lumino_note_core::font_scanner::FontInfo],
) -> crate::Element<'a> {
    let t = settings_translations(settings.display.language);
    let palette = window.theme.extended_palette();

    // 设置内容（复用现有的 settings::view）
    let settings_content = settings::view(settings, window, system_fonts);

    // 确认按钮
    let confirm_button = button(text(t.confirm).size(14))
        .on_press(Message::SettingsDialog(SettingsDialogAction::CloseDialog))
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
        row![space().width(Length::Fill), confirm_button].align_y(Alignment::Center),
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

    if settings.editing.tempo_custom_open {
        // 自定义 BPM 上限弹窗：悬浮层（遮罩 + 居中输入卡片）
        Stack::new()
            .push(dialog_content)
            .push(custom_bpm_backdrop(settings))
            .push(custom_bpm_card_layer(settings))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        dialog_content.into()
    }
}

/// 自定义 BPM 上限弹窗的遮罩层：点击外部区域关闭
fn custom_bpm_backdrop<'a>(_settings: &settings::SettingsPanel) -> crate::Element<'a> {
    container(
        mouse_area(Space::new().width(Length::Fill).height(Length::Fill)).on_press(
            Message::Settings(crate::settings::Event::TempoMaxBpmCustomClose),
        ),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme: &iced_core::Theme| container::Style {
        background: Some(iced_core::Background::Color(Color::from_rgba(
            0.0, 0.0, 0.0, 0.45,
        ))),
        ..Default::default()
    })
    .into()
}

/// 自定义 BPM 上限弹窗的输入卡片层（居中悬浮）
fn custom_bpm_card_layer<'a>(settings: &settings::SettingsPanel) -> crate::Element<'a> {
    let t = settings_translations(settings.display.language);

    let confirm_btn = button(text(t.confirm).size(13))
        .on_press(Message::Settings(
            crate::settings::Event::TempoMaxBpmCustomConfirm,
        ))
        .padding([6, 20]);
    let cancel_btn = button(text(t.cancel).size(13))
        .on_press(Message::Settings(
            crate::settings::Event::TempoMaxBpmCustomClose,
        ))
        .padding([6, 20]);

    let card = container(
        column![
            text(t.editing_tempo_custom_title)
                .size(14)
                .style(|theme: &iced_core::Theme| text::Style {
                    color: Some(theme.extended_palette().background.neutral.text),
                }),
            space().height(12),
            text_input(
                t.editing_tempo_custom_placeholder,
                &settings.editing.tempo_custom_input
            )
            .on_input(|v| Message::Settings(crate::settings::Event::TempoMaxBpmCustomInput(v)))
            .padding([6, 10])
            .width(Length::Fixed(220.0)),
            space().height(16),
            row![confirm_btn, space().width(8), cancel_btn]
                .spacing(4)
                .align_y(Alignment::Center),
        ]
        .align_x(Alignment::Start),
    )
    .padding(20)
    .style(|theme: &iced_core::Theme| container::Style {
        background: Some(iced_core::Background::Color(
            theme.extended_palette().background.base.color,
        )),
        border: iced_core::Border::default()
            .rounded(8)
            .width(1)
            .color(theme.extended_palette().background.strong.color),
        ..Default::default()
    });

    // 卡片区域用 mouse_area 吞掉点击，避免点击卡片空白处穿透触发遮罩关闭
    let card = mouse_area(card).on_press(Message::Null);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced_core::alignment::Horizontal::Center)
        .align_y(iced_core::alignment::Vertical::Center)
        .into()
}
