use iced_aw::{
    Menu, MenuBar,
    menu::Item,
    style::menu_bar,
};
use iced_core::{
    Alignment, Background, Border, Color, Length, theme::Base
};
use iced_widget::{button, column, container, row, space, text};

use crate::{
    message,
    resources::icon,
    Message,
    Theme,
    Renderer,
    Element,
};

use lumino_core::{
    Event,
    event,
};

#[derive(Debug, Clone)]
pub enum MenuKind {
    File,
    Edit,
    View,
    Help,
}

impl std::fmt::Display for MenuKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone)]
pub enum MenuItem {
    // Action(Event, Fn) or something like this for i18n.
    Action(Event),
    Separator,
    // Submenu(Vec<MenuItem>, Fn)
    Submenu(Vec<MenuItem>, String)
}

#[derive(Debug, Clone)]
pub struct MenuConfig {
    pub kind: MenuKind,
    pub items: Vec<MenuItem>,
}

fn file_menu() -> MenuConfig {
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::File,
        items: vec![
            Action(event!(Menu.File.New)),
            Action(event!(Menu.File.Open)),
            Action(event!(Menu.File.Save)),
            Action(event!(Menu.File.Close)),
            Separator,
            Action(event!(Menu.File.ImportMidi)),
            Separator,
            Action(event!(Menu.File.Settings)),
            Separator,
            Action(event!(Menu.File.Exit)),

        ]
    }
}

fn edit_menu() -> MenuConfig {
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::Edit,
        items: vec![
            Action(event!(Menu.Edit.Undo)),
            Action(event!(Menu.Edit.Redo)),
            Separator,
            Action(event!(Menu.Edit.Cut)),
            Action(event!(Menu.Edit.Copy)),
            Action(event!(Menu.Edit.Paste)),
            Action(event!(Menu.Edit.SelectAll)),
            Separator,
            Action(event!(Menu.Edit.Find)),

        ]
    }
}

fn view_menu() -> MenuConfig {
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::View,
        items: vec![
            Submenu(
                Theme::ALL
                    .iter().map(|r| Action(event!(Menu.View.Theme(r.name().into()))))
                    .collect::<Vec<_>>(),
                "Theme".into()
            )
        ]
    }
}

fn help_menu() -> MenuConfig {
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::Help,
        items: vec![
            Action(event!(Menu.Help.About)),
        ]
    }
}

fn menus() -> [MenuConfig; 4] {
    [file_menu(), edit_menu(), view_menu(), help_menu()]
}

pub fn view<'a>() -> Element<'a> {
    let menus = menus()
        .iter()
        .map(|cfg| Item::with_menu(
            menu_button(cfg.kind.to_string()),
            // DO NOT REMOVE `width(200)`!
            // Removing it causes a panic. idk why.
            // Use offset to align it with titlebar.
            Menu::new(menu_items(&cfg.items)).width(200).offset(9.0),
        ))
        .collect::<Vec<_>>();

    let inner = MenuBar::new(menus)
        .close_on_background_click_global(true)
        .close_on_item_click_global(true)
        .height(Length::Fill)
        .spacing(1)
        .style(|theme: &Theme, status| menu_bar::Style {
            bar_background: Background::Color(Color::TRANSPARENT),
            // Use the default style from iced_aw.
            // `..Default::default()` simply messes up the styles.
            ..menu_bar::primary(theme, status)
        });

    row![
        inner,
        space().width(Length::Fill)
    ].into()
}

fn menu_items<'a>(items: &Vec<MenuItem>) -> Vec<Item<'a, Message, Theme, Renderer>> {
    items.iter().map(|item| {
        let inner: Element<'a> = match item {
            MenuItem::Action(r) => base_button(
                format!("{r:?}"),
                Some(Message::Core(r.clone()))
            ),
            MenuItem::Separator => base_split(),
            MenuItem::Submenu(r, n) => return Item::with_menu(
                submenu_button(n),
                Menu::new(menu_items(r)).width(400).offset(12.0)
            ),
        };
        Item::new(inner)
    })
    .collect::<Vec<_>>()
}

fn submenu_button<'a>(label: impl Into<String>) -> Element<'a> {
    let icon = icon(icon::AngleRight)
        .width(14)
        .height(14);
    let inner = row![
        text(label.into())
            .size(14.0)
            .width(Length::Fill),
        container(icon)
            .height(20)
            .align_y(Alignment::Center)
    ].into();
    button_template(inner, message::null())
        .padding([2, 8])
        .into()
}

fn menu_button<'a>(label: impl Into<String>) -> Element<'a> {
    let inner = text(label.into()).size(14.0).into();
    button_template(inner, message::null())
        .padding([2, 8])
        .into()
}

fn base_button<'a>(label: impl Into<String>, msg: Option<Message>) -> Element<'a> {
    let inner = text(label.into()).size(14.0).into();
    button_template(inner, msg.unwrap_or(message::null()))
        .width(Length::Fill)
        .into()
}

fn button_template<'a>(inner: Element<'a>, msg: Message) -> button::Button<'a, Message, Theme, Renderer> {
    button(inner)
        .style(|theme: &Theme, status| {
            use button::Status::*;

            let palette = theme.extended_palette();
            let background = match status {
                Hovered => palette.background.weaker.color,
                Pressed => palette.background.weak.color,
                _ => Color::TRANSPARENT,
            };

            button::Style {
                border: Border::default().rounded(4),
                text_color: palette.background.neutral.text,
                ..Default::default()
            }
            .with_background(background)
        })
        .on_press(msg)
}

fn base_split<'a>() -> Element<'a> {
    let inner = container(space())
        .width(Length::Fill)
        .height(1)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(Background::Color(palette.background.strongest.color)),
                ..Default::default()
            }
        });

    // Manually apply the `margin` style.
    column![space().height(4), inner, space().height(4)]
        .width(Length::Fill)
        .into()
}
