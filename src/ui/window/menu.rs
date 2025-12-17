use iced::{Background, Border, Color, Element, Length, Theme, widget::{Button, Text, button}};
use iced_aw::{Menu, MenuBar, menu::Item, quad};

use crate::app::{
    Message,
    window::menu::{
        EditAction, FileAction, HelpAction, MenuAction, ViewAction
    }, window::WindowEvent,
};

#[derive(Debug, Clone)]
enum MenuType {
    File,
    Edit,
    View,
    Help
}

#[derive(Debug, Clone)]
pub enum MenuItem {
    Action(MenuAction),
    Separator,
}

#[derive(Debug, Clone)]
struct MenuConfig {
    class: MenuType,
    items: Vec<MenuItem>,
}

fn file_menu() -> MenuConfig {
    use FileAction::*;
    use MenuAction::File;
    use MenuItem::*;
    MenuConfig {
        class: MenuType::File,
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
        ]
    }
}

fn edit_menu() -> MenuConfig {
    use EditAction::*;
    use MenuAction::Edit;
    use MenuItem::*;

    MenuConfig {
        class: MenuType::Edit,
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
    use ViewAction::*;
    use MenuAction::View;
    use MenuItem::*;

    MenuConfig {
        class: MenuType::View,
        /* TODO */
        items: vec![],
    }
}

fn help_menu() -> MenuConfig {
    use HelpAction::*;
    use MenuAction::Help;
    use MenuItem::*;

    MenuConfig {
        class: MenuType::Help,
        /* TODO */
        items: vec![],
    }
}

pub fn view<'a>() -> Element<'a, Message> {
    todo!()
    // let menus = vec![
    //     file_menu(),
    //     edit_menu(),
    //     // view_menu(),
    //     // help_menu(),
    // ];

    // let menus = menus.iter().map(|cfg| {
    //     let items = cfg.items.iter().map(|item| {
    //         let item: Element<'a, Message> = match item {
    //             MenuItem::Action(r) => {
    //                 Button::new(
    //                     Text::new(format!("{r:?}"))
    //                 )
    //                     .on_press(Message::Window(
    //                         WindowEvent::Menu(*r)
    //                     ))
    //                     .into()
    //             },
    //             MenuItem::Separator => {
    //                 quad::Quad {
    //                     width: Length::Fill,
    //                     height: 1.0.into(),
    //                     quad_color: Background::Color(
    //                         Color::from_rgba(0.0, 0.0, 0.0, 0.1)
    //                     ),
    //                     ..Default::default()
    //                 }.into()
    //             }
    //         };
    //         Item::new(item)
    //     }).collect::<Vec<_>>();
    //     let menu = Menu::new(items).width(200);
    //     Item::with_menu(
    //         Button::new(
    //             Text::new(format!("{:?}", cfg.class)).size(14)
    //         )
    //             .style(|theme: &Theme, status| {
    //                 use button::Status::*;
    //                 let palette = theme.extended_palette();
    //                 let base = button::Style {
    //                     text_color: Color::WHITE,
    //                     ..Default::default()
    //                 };
    //                 match status {
    //                     Active => base.with_background(Color::TRANSPARENT),
    //                     Hovered => base.with_background(Color::from_rgb(
    //                         palette.primary.weak.color.r * 1.2,
    //                         palette.primary.weak.color.g * 1.2,
    //                         palette.primary.weak.color.b * 1.2,
    //                     )),
    //                     Disabled => base.with_background(Color::from_rgb(0.5, 0.5, 0.5)),
    //                     Pressed => base.with_background(palette.primary.weak.color),
    //                     // Status::Disabled => base.with_background(Color::from_rgb(1.0, 0.0, 0.0)),
    //                     // Status::Pressed => base.with_background(Color::from_rgb(0.0, 1.0, 0.0)),
    //                     // _ => iced::widget::button::primary(theme, status)
    //                 }
    //             }),
    //         menu
    //     )
    // }).collect::<Vec<_>>();

    // MenuBar::new(menus)
    //     .spacing(1)
    //     .close_on_background_click_global(true)
    //     .close_on_item_click_global(true)
    //     .into()
}
