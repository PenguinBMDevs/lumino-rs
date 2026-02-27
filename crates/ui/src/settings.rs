use iced_core::{Alignment, Border, Length, Padding};
use iced_widget::{button, column, container, row, text};

use crate::{
    Element, Message, Theme,
    resources::icon::{self, Icon},
    window,
};

/// 设置面板相关的常量定义
mod constants {
    // 图标尺寸
    pub const ICON_SIZE_SMALL: u32 = 18;

    // 文本尺寸 (使用 f32 以兼容 iced_core::Pixels)
    pub const TEXT_SIZE_LABEL: f32 = 14.0;
    pub const TEXT_SIZE_ARROW: f32 = 12.0;
    pub const TEXT_SIZE_TITLE: f32 = 18.0;
    pub const TEXT_SIZE_CONTENT: f32 = 14.0;

    // 布局尺寸
    pub const MENU_WIDTH: f32 = 200.0;
    pub const ICON_CONTAINER_WIDTH: f32 = 24.0;

    // 间距
    pub const SPACING_ICON_LABEL: f32 = 8.0;
    pub const SPACING_CONTENT: f32 = 10.0;
    pub const SPACING_MAIN: f32 = 16.0;
    pub const SPACING_MENU_CONTENT: f32 = 0.0;

    // 内边距
    pub const PADDING_ITEM_VERTICAL: f32 = 12.0;
    pub const PADDING_ITEM_HORIZONTAL: f32 = 16.0;
    pub const PADDING_MENU: f32 = 1.0;
    pub const PADDING_CONTENT: f32 = 20.0;

    // 圆角
    pub const BORDER_RADIUS_MENU: f32 = 16.0;
    pub const BORDER_RADIUS_CONTENT: f32 = 21.0;
    pub const BORDER_WIDTH: f32 = 1.0;

    // 阴影
    pub const SHADOW_COLOR_MENU: [f32; 4] = [0.0, 0.0, 0.0, 0.15];
    pub const SHADOW_OFFSET_MENU: (f32, f32) = (0.0, 4.0);
    pub const SHADOW_BLUR_MENU: f32 = 8.0;

    pub const SHADOW_COLOR_CONTENT: [f32; 4] = [0.0, 0.0, 0.0, 0.25];
    pub const SHADOW_OFFSET_CONTENT: (f32, f32) = (0.0, 4.0);
    pub const SHADOW_BLUR_CONTENT: f32 = 4.0;
}

use constants::*;

#[derive(Debug, Clone)]
pub enum Event {
    MenuSelected(usize),
}

#[derive(Debug, Clone)]
pub struct SettingsPanel {
    pub selected_menu_index: usize,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            selected_menu_index: 0,
        }
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::MenuSelected(idx) => {
                self.selected_menu_index = idx;
            }
        }
    }
}

/// 渲染设置面板主视图
pub fn view<'a>(settings: &SettingsPanel, window: &window::Window) -> Element<'a> {
    let menu_items = create_menu_items();

    let menu_list = render_menu_list(settings, window, &menu_items);
    let content_area = render_content_area();

    let main_content = row![
        menu_list,
        iced_widget::space().width(SPACING_MAIN),
        content_area,
    ]
    .spacing(SPACING_MENU_CONTENT)
    .padding(PADDING_CONTENT);

    container(main_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(create_main_container_style())
        .into()
}

/// 创建菜单项列表
fn create_menu_items() -> Vec<(&'static str, Icon)> {
    vec![
        ("常规", Icon::Gear),
        ("音频", Icon::WaveForm),
        ("界面", Icon::FolderTree),
        ("快捷键", Icon::Clock),
        ("关于", Icon::GitHub),
    ]
}

/// 渲染菜单列表
fn render_menu_list<'a>(
    settings: &SettingsPanel,
    window: &window::Window,
    menu_items: &[(&'static str, Icon)],
) -> iced_widget::Container<'a, Message, Theme, crate::Renderer> {
    let mut col = column![]
        .spacing(SPACING_MENU_CONTENT)
        .padding(PADDING_MENU);

    for (idx, (label, icon)) in menu_items.iter().enumerate() {
        let menu_item = render_menu_item(settings, window, idx, label, *icon);
        col = col.push(menu_item);
    }

    container(col)
        .width(MENU_WIDTH)
        .height(Length::Fill)
        .style(create_menu_container_style())
}

/// 渲染单个菜单项
fn render_menu_item<'a>(
    settings: &SettingsPanel,
    window: &window::Window,
    index: usize,
    label: &'static str,
    icon: Icon,
) -> iced_widget::Button<'a, Message, Theme, crate::Renderer> {
    let is_selected = index == settings.selected_menu_index;

    let icon_el =
        icon::view_with_size_and_theme(icon, ICON_SIZE_SMALL, ICON_SIZE_SMALL, Some(&window.theme));

    let label_text = render_menu_label(label, is_selected);
    let arrow = render_menu_arrow();

    let item_row = row![
        container(icon_el)
            .width(ICON_CONTAINER_WIDTH)
            .align_x(Alignment::Center),
        label_text,
        arrow,
    ]
    .spacing(SPACING_ICON_LABEL)
    .align_y(Alignment::Center)
    .padding(create_item_padding());

    button(item_row)
        .width(Length::Fill)
        .on_press(Message::Settings(Event::MenuSelected(index)))
        .style(create_menu_button_style(is_selected))
}

/// 渲染菜单标签
fn render_menu_label<'a>(
    label: &'static str,
    is_selected: bool,
) -> iced_widget::Text<'a, Theme, crate::Renderer> {
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
        })
}

/// 渲染菜单箭头
fn render_menu_arrow<'a>() -> iced_widget::Text<'a, Theme, crate::Renderer> {
    text(">").size(TEXT_SIZE_ARROW).style(|theme: &Theme| {
        let palette = theme.extended_palette();
        text::Style {
            color: Some(palette.background.weak.text),
        }
    })
}

/// 创建菜单项内边距
fn create_item_padding() -> Padding {
    Padding::new(PADDING_ITEM_VERTICAL)
        .left(PADDING_ITEM_HORIZONTAL)
        .right(PADDING_ITEM_HORIZONTAL)
}

/// 创建菜单按钮样式
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

/// 创建菜单容器样式
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

/// 渲染内容区域
fn render_content_area<'a>() -> iced_widget::Container<'a, Message, Theme, crate::Renderer> {
    container(
        column![
            text("设置")
                .size(TEXT_SIZE_TITLE)
                .style(create_content_text_style()),
            iced_widget::space().height(20),
            text("设置内容区域")
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_CONTENT)
        .padding(PADDING_CONTENT),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(create_content_container_style())
}

/// 创建内容文本样式
fn create_content_text_style() -> impl Fn(&Theme) -> text::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        text::Style {
            color: Some(palette.background.base.text),
        }
    }
}

/// 创建占位符文本样式
fn create_placeholder_text_style() -> impl Fn(&Theme) -> text::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        text::Style {
            color: Some(palette.background.weak.text),
        }
    }
}

/// 创建内容容器样式
fn create_content_container_style() -> impl Fn(&Theme) -> container::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(iced_core::Background::Color(palette.background.base.color)),
            border: Border::default()
                .rounded(BORDER_RADIUS_CONTENT)
                .width(BORDER_WIDTH)
                .color(palette.background.strong.color),
            shadow: iced_core::Shadow {
                color: iced_core::Color::from_rgba(
                    SHADOW_COLOR_CONTENT[0],
                    SHADOW_COLOR_CONTENT[1],
                    SHADOW_COLOR_CONTENT[2],
                    SHADOW_COLOR_CONTENT[3],
                ),
                offset: iced_core::Vector::new(SHADOW_OFFSET_CONTENT.0, SHADOW_OFFSET_CONTENT.1),
                blur_radius: SHADOW_BLUR_CONTENT,
            },
            text_color: Some(palette.background.base.text),
            snap: false,
        }
    }
}

/// 创建主容器样式
fn create_main_container_style() -> impl Fn(&Theme) -> container::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(iced_core::Background::Color(
                palette.background.weakest.color,
            )),
            text_color: Some(palette.background.base.text),
            snap: false,
            ..Default::default()
        }
    }
}
