use iced_aw::{Menu, MenuBar, menu::Item, style::menu_bar};
use iced_core::{Alignment, Background, Border, Color, Length};
use iced_widget::{button, column, container, row, space, text};

use crate::{Element, Message, Renderer, Theme, message, resources::icon};

use lumino_core::{Event, event};

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
    // 用于 i18n 的 Action(Event, Fn) 或类似结构
    Action(Event),
    Separator,
    // 子菜单(Vec<MenuItem>, Fn)
    Submenu(Vec<MenuItem>, String),
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
            Action(event!(Menu.File.ImportFiles)),
            Separator,
            Action(event!(Menu.File.Settings)),
            Separator,
            Action(event!(Menu.File.Exit)),
        ],
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
        ],
    }
}

fn view_menu() -> MenuConfig {
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::View,
        items: vec![
            Action(event!(Menu.View.ZoomIn)),
            Action(event!(Menu.View.ZoomOut)),
            Action(event!(Menu.View.ZoomReset)),
        ],
    }
}

fn help_menu() -> MenuConfig {
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::Help,
        items: vec![Action(event!(Menu.Help.About))],
    }
}

fn menus() -> [MenuConfig; 4] {
    [file_menu(), edit_menu(), view_menu(), help_menu()]
}

pub fn view<'a>() -> Element<'a> {
    let menus = menus()
        .iter()
        .map(|cfg| {
            Item::with_menu(
                menu_button(cfg.kind.to_string()),
                // 不要删除 'width(200)'！
                // 删除它会导致 panic。原因未知
                // 使用 offset 来与标题栏对齐
                Menu::new(menu_items(&cfg.items)).width(200).offset(9.0),
            )
        })
        .collect::<Vec<_>>();

    let inner = MenuBar::new(menus)
        .close_on_background_click_global(true)
        .close_on_item_click_global(true)
        .height(Length::Fill)
        .spacing(1)
        .style(|theme: &Theme, status| menu_bar::Style {
            bar_background: Background::Color(Color::TRANSPARENT),
            // 使用 iced_aw 的默认样式
            // '..Default::default()' 会破坏样式
            ..menu_bar::primary(theme, status)
        });

    inner.into()
}

fn menu_items<'a>(items: &[MenuItem]) -> Vec<Item<'a, Message, Theme, Renderer>> {
    items
        .iter()
        .map(|item| {
            let inner: Element<'a> = match item {
                MenuItem::Action(r) => {
                    // 点击菜单项时发送菜单关闭消息
                    let msg = Message::Core(r.clone());
                    base_button(format!("{r:?}"), Some(msg))
                }
                MenuItem::Separator => base_split(),
                MenuItem::Submenu(r, n) => {
                    return Item::with_menu(
                        submenu_button(n),
                        Menu::new(menu_items(r)).width(400).offset(12.0),
                    );
                }
            };
            Item::new(inner)
        })
        .collect::<Vec<_>>()
}

fn submenu_button<'a>(label: impl Into<String>) -> Element<'a> {
    let icon: Element<'a> = container(icon(icon::AngleRight))
        .width(12)
        .height(12)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into();
    let inner = row![
        text(label.into()).size(14.0).width(Length::Fill),
        container(icon)
            .height(20)
            .padding(3)
            .align_y(Alignment::Center)
    ]
    .into();
    button_template(inner, message::null())
        .padding([2, 8])
        .into()
}

fn menu_button<'a>(label: impl Into<String>) -> Element<'a> {
    let inner = text(label.into()).size(14.0).into();
    // 菜单按钮点击时打开菜单
    button_template(inner, message::Message::MenuStateChanged(true))
        .padding([2, 8])
        .into()
}

fn base_button<'a>(label: impl Into<String>, msg: Option<Message>) -> Element<'a> {
    let inner = text(label.into()).size(14.0).into();
    button_template(inner, msg.unwrap_or(message::null()))
        .width(Length::Fill)
        .into()
}

fn button_template<'a>(
    inner: Element<'a>,
    msg: Message,
) -> button::Button<'a, Message, Theme, Renderer> {
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

    // 手动应用 `margin` 样式
    column![space().height(4), inner, space().height(4)]
        .width(Length::Fill)
        .into()
}
