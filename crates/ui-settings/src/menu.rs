//! 设置面板菜单渲染

use iced_core::{Alignment, Border, Length, Padding};
use iced_widget::{button, column, container, row, scrollable, text};

use super::{Event, SettingsPanel, components::*};
use lumino_core::i18n::{Language, settings_translations};
use lumino_ui_core::{
    Message, Theme,
    resources::icon::{self, Icon},
    window,
};

pub(super) fn create_menu_items(lang: Language) -> Vec<(&'static str, Icon)> {
    let translations = settings_translations(lang);
    vec![
        (translations.general, Icon::Gear),
        (translations.audio, Icon::WaveForm),
        (translations.ui, Icon::FolderTree),
        (translations.shortcuts, Icon::Clock),
        (translations.onion_skin, Icon::Eye),
        (translations.palette, Icon::Arrangement),
        (translations.editing, Icon::Pencil),
        (translations.about, Icon::LogoInApp),
    ]
}

pub(super) fn render_menu_list<'a>(
    settings: &SettingsPanel,
    window: &window::Window,
    menu_items: &[(&'static str, Icon)],
) -> iced_widget::Container<'a, Message, Theme, lumino_ui_core::Renderer> {
    let mut col = column![]
        .spacing(SPACING_MENU_CONTENT)
        .padding(PADDING_MENU);

    for (idx, (label, icon)) in menu_items.iter().enumerate() {
        let menu_item = render_menu_item(settings, window, idx, label, *icon);
        col = col.push(menu_item);
    }

    let scrolled = scrollable(col).width(Length::Fill).height(Length::Fill);

    container(scrolled)
        .width(MENU_WIDTH)
        .height(Length::Fill)
        .style(create_menu_container_style())
}

fn render_menu_item<'a>(
    settings: &SettingsPanel,
    window: &window::Window,
    index: usize,
    label: &'static str,
    icon: Icon,
) -> iced_widget::Button<'a, Message, Theme, lumino_ui_core::Renderer> {
    let is_selected = index == settings.selected_menu_index;

    let icon_el =
        icon::view_with_size_and_theme(icon, ICON_SIZE_SMALL, ICON_SIZE_SMALL, Some(&window.theme));

    let label_text =
        text(label)
            .size(TEXT_SIZE_LABEL)
            .width(Length::Fill)
            .style(move |theme: &Theme| {
                let palette = theme.extended_palette();
                text::Style {
                    color: Some(if is_selected {
                        palette.primary.strong.color
                    } else {
                        palette.background.base.text
                    }),
                }
            });

    let arrow = text(">").size(TEXT_SIZE_ARROW).style(|theme: &Theme| {
        let palette = theme.extended_palette();
        text::Style {
            color: Some(palette.background.weak.text),
        }
    });

    let item_row = row![
        container(icon_el)
            .width(ICON_CONTAINER_WIDTH)
            .align_x(Alignment::Center),
        label_text,
        arrow,
    ]
    .spacing(SPACING_ICON_LABEL)
    .align_y(Alignment::Center)
    .padding(
        Padding::new(PADDING_ITEM_VERTICAL)
            .left(PADDING_ITEM_HORIZONTAL)
            .right(PADDING_ITEM_HORIZONTAL),
    );

    button(item_row)
        .width(Length::Fill)
        .on_press(Message::Settings(Event::MenuSelected(index)))
        .style(create_menu_button_style(is_selected))
}

fn create_menu_button_style(
    is_selected: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |theme: &Theme, status| {
        let palette = theme.extended_palette();
        let bg = if is_selected {
            palette.background.weak.color
        } else if status == button::Status::Hovered {
            palette.background.weakest.color
        } else {
            iced_core::Color::TRANSPARENT
        };

        button::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: Border::default(),
            text_color: palette.background.base.text,
            shadow: iced_core::Shadow::default(),
            snap: false,
        }
    }
}

fn create_menu_container_style() -> impl Fn(&Theme) -> container::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(iced_core::Background::Color(palette.background.weak.color)),
            border: Border::default()
                .rounded(BORDER_RADIUS_MENU)
                .width(BORDER_WIDTH)
                .color(palette.background.strong.color),
            shadow: iced_core::Shadow {
                color: iced_core::Color::from_rgba(
                    SHADOW_COLOR_MENU[0],
                    SHADOW_COLOR_MENU[1],
                    SHADOW_COLOR_MENU[2],
                    SHADOW_COLOR_MENU[3],
                ),
                offset: iced_core::Vector::new(SHADOW_OFFSET_MENU.0, SHADOW_OFFSET_MENU.1),
                blur_radius: SHADOW_BLUR_MENU,
            },
            text_color: Some(palette.background.base.text),
            snap: false,
        }
    }
}
