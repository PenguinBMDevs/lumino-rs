use iced_aw::{Menu, MenuBar, menu::Item, style::menu_bar};
use iced_core::{Alignment, Background, Border, Color, Length};
use iced_widget::{button, column, container, row, space, text};

use crate::{Element, Message, Renderer, Theme, message, resources::icon};

use crate::event::Event;

const MENU_WIDTH: f32 = 200.0;

#[derive(Debug, Clone)]
pub enum MenuKind {
    File,
    Edit,
    View,
    Help,
}

impl MenuKind {
    /// 获取菜单类型的中文显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::File => "文件",
            Self::Edit => "编辑",
            Self::View => "视图",
            Self::Help => "帮助",
        }
    }
}

impl std::fmt::Display for MenuKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
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

pub fn file_menu() -> MenuConfig {
    use crate::event::menu::file;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::File,
        items: vec![
            Action(crate::event::Event::menu_file(file::Event::new_file())),
            Action(crate::event::Event::menu_file(file::Event::open())),
            Action(crate::event::Event::menu_file(file::Event::save())),
            Action(crate::event::Event::menu_file(file::Event::close())),
            Separator,
            Action(crate::event::Event::menu_file(file::Event::import_files())),
            Separator,
            Submenu(
                vec![
                    Action(crate::event::Event::menu_file(
                        file::Event::export_project_archive(),
                    )),
                    Action(crate::event::Event::menu_file(
                        file::Event::export_project_folder(),
                    )),
                ],
                "导出工程".into(),
            ),
            Action(crate::event::Event::menu_file(file::Event::audio_export())),
            Separator,
            Action(crate::event::Event::menu_file(
                file::Event::project_settings(),
            )),
            Separator,
            Action(crate::event::Event::menu_file(file::Event::settings())),
            Separator,
            Action(crate::event::Event::menu_file(file::Event::exit())),
        ],
    }
}

pub fn edit_menu() -> MenuConfig {
    use crate::event::menu::edit;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::Edit,
        items: vec![
            Action(crate::event::Event::menu_edit(edit::Event::undo())),
            Action(crate::event::Event::menu_edit(edit::Event::redo())),
            Separator,
            Action(crate::event::Event::menu_edit(edit::Event::cut())),
            Action(crate::event::Event::menu_edit(edit::Event::copy())),
            Action(crate::event::Event::menu_edit(edit::Event::paste())),
            Action(crate::event::Event::menu_edit(edit::Event::select_all())),
            Separator,
            Action(crate::event::Event::menu_edit(edit::Event::find())),
        ],
    }
}

pub fn view_menu() -> MenuConfig {
    use crate::event::menu::view;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::View,
        items: vec![
            Action(crate::event::Event::menu_view(view::Event::zoom_in())),
            Action(crate::event::Event::menu_view(view::Event::zoom_out())),
            Action(crate::event::Event::menu_view(view::Event::zoom_reset())),
        ],
    }
}

pub fn help_menu() -> MenuConfig {
    use crate::event::menu::help;
    use MenuItem::*;
    MenuConfig {
        kind: MenuKind::Help,
        items: vec![Action(crate::event::Event::menu_help(help::Event::about()))],
    }
}

pub fn menus() -> [MenuConfig; 4] {
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
                Menu::new(menu_items(&cfg.items))
                    .width(MENU_WIDTH)
                    .offset(9.0),
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
                    base_button(event_display_name(r), Some(msg))
                }
                MenuItem::Separator => base_split(),
                MenuItem::Submenu(r, n) => {
                    return Item::with_menu(
                        submenu_button(n),
                        Menu::new(menu_items(r)).width(MENU_WIDTH).offset(12.0),
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
        text(label.into())
            .size(14.0)
            .width(Length::Fill)
            .align_x(iced_core::alignment::Horizontal::Left),
        container(icon)
            .width(Length::Fixed(16.0))
            .height(20)
            .padding(3)
            .align_y(Alignment::Center)
    ]
    .width(Length::Fill)
    .into();
    button_template(inner, message::null())
        .width(Length::Fill)
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

/// 获取事件的友好显示名称
fn event_display_name(event: &Event) -> String {
    use crate::event::menu::{
        edit::Event as EditEvent, file::Event as FileEvent, help::Event as HelpEvent,
        view::Event as ViewEvent,
    };

    match event {
        Event::Menu(menu_event) => match menu_event {
            crate::event::menu::Event::File(file_event) => match file_event {
                FileEvent::New => "新建".to_string(),
                FileEvent::Open => "打开".to_string(),
                FileEvent::Save => "保存".to_string(),
                FileEvent::Close => "关闭".to_string(),
                FileEvent::ImportFiles => "导入文件".to_string(),
                FileEvent::ExportProjectArchive => "导出为单文件".to_string(),
                FileEvent::ExportProjectFolder => "导出为文件夹".to_string(),
                FileEvent::AudioExport => "导出音频".to_string(),
                FileEvent::ProjectSettings => "工程设置".to_string(),
                FileEvent::Settings => "设置".to_string(),
                FileEvent::Exit => "退出".to_string(),
                _ => format!("{file_event:?}"),
            },
            crate::event::menu::Event::Edit(edit_event) => match edit_event {
                EditEvent::Undo => "撤销".to_string(),
                EditEvent::Redo => "重做".to_string(),
                EditEvent::Cut => "剪切".to_string(),
                EditEvent::Copy => "复制".to_string(),
                EditEvent::Paste => "粘贴".to_string(),
                EditEvent::SelectAll => "全选".to_string(),
                EditEvent::Find => "查找".to_string(),
            },
            crate::event::menu::Event::View(view_event) => match view_event {
                ViewEvent::ZoomIn => "放大".to_string(),
                ViewEvent::ZoomOut => "缩小".to_string(),
                ViewEvent::ZoomReset => "重置缩放".to_string(),
                _ => format!("{view_event:?}"),
            },
            crate::event::menu::Event::Help(help_event) => match help_event {
                HelpEvent::About => "关于".to_string(),
            },
        },
        Event::Window(window_event) => format!("{window_event:?}"),
    }
}
