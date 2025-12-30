use iced::{
    Background, Border, Color, Element, Length, Theme,
    widget::{button, column, container, space, text},
};
use iced_aw::{
    Menu, MenuBar,
    menu::Item,
    style::menu_bar,
};

use crate::app::{
    Message,
    window::{
        self,
        menu::{EditAction, FileAction, HelpAction, MenuAction, ViewAction},
    },
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
    Action(MenuAction),
    Separator,
}

#[derive(Debug, Clone)]
pub struct MenuConfig {
    pub kind: MenuKind,
    pub items: Vec<MenuItem>,
}

fn file_menu() -> MenuConfig {
    use FileAction::*;
    use MenuAction::File;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::File,
        items: vec![
            Action(File(New)),
            Action(File(Open)),
            Action(File(Save)),
            Action(File(Close)),
            Separator,
            Action(File(ImportMidi)),
            Separator,
            Action(File(Settings)),
            Separator,
            Action(File(Exit)),
        ],
    }
}

fn edit_menu() -> MenuConfig {
    use EditAction::*;
    use MenuAction::Edit;
    use MenuItem::*;

    MenuConfig {
        kind: MenuKind::Edit,
        items: vec![
            Action(Edit(Undo)),
            Action(Edit(Redo)),
            Separator,
            Action(Edit(Cut)),
            Action(Edit(Copy)),
            Action(Edit(Paste)),
            Action(Edit(SelectAll)),
            Separator,
            Action(Edit(Find)),
        ],
    }
}

fn view_menu() -> MenuConfig {
    use MenuAction::View;
    use MenuItem::*;
    use ViewAction::*;

    MenuConfig {
        kind: MenuKind::View,
        /* TODO */
        items: vec![Action(View(Light)), Action(View(Dark))],
    }
}

fn help_menu() -> MenuConfig {
    use HelpAction::*;
    use MenuAction::Help;
    use MenuItem::*;

    MenuConfig {
        kind: MenuKind::Help,
        items: vec![Action(Help(About))],
    }
}

pub fn menus() -> [MenuConfig; 4] {
    [file_menu(), edit_menu(), view_menu(), help_menu()]
}

pub fn view<'a>() -> Element<'a, Message> {
    let menus = menus()
        .iter()
        .map(|cfg| {
            // Inline to avoid verbose function return type definitions.
            let items = cfg
                .items
                .iter()
                .map(|item| {
                    let inner: Element<'a, Message> = match item {
                        MenuItem::Action(r) => {
                            base_button(r.to_string(), Message::Window(window::Event::Menu(*r)))
                                .width(Length::Fill)
                                .into()
                        }
                        MenuItem::Separator => base_split(),
                    };
                    Item::new(inner)
                })
                .collect::<Vec<_>>();
            Item::with_menu(
                base_button(cfg.kind.to_string(), Message::Null).padding([2, 8]),
                // DO NOT REMOVE `width(200)`!
                // Removing it causes a panic. idk why.
                // Use offset to make it flush with the titlebar.
                Menu::new(items).width(200).offset(9.0),
            )
        })
        .collect::<Vec<_>>();

    MenuBar::new(menus)
        .close_on_background_click_global(true)
        .close_on_item_click_global(true)
        .spacing(1)
        .style(|theme: &Theme, status| menu_bar::Style {
            bar_background: Background::Color(Color::TRANSPARENT),
            // Use the default style from iced_aw.
            // `..Default::default()` simply messes up the styles.
            ..menu_bar::primary(theme, status)
        })
        .into()
}

fn base_button<'a>(label: impl Into<String>, msg: Message) -> button::Button<'a, Message> {
    let inner = text(label.into()).size(14.0);
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

fn base_split<'a>() -> Element<'a, Message> {
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
