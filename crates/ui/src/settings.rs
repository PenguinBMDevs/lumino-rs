use iced_core::{Alignment, Border, Length, Padding};
use iced_widget::{button, column, container, pick_list, row, text, text_input};

use crate::{
    Element, Message, Theme,
    resources::icon::{self, Icon},
    window,
};
use lumino_core::storage::config::SynthBackend;

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
    SynthBackendChanged(SynthBackend),
    SoundfontPathChanged(String),
    BrowseSoundfont,
    NativeTitlebarChanged(bool),
}

#[derive(Debug, Clone)]
pub struct SettingsPanel {
    pub selected_menu_index: usize,
    pub synth_backend: SynthBackend,
    pub soundfont_path: String,
    pub use_native_titlebar: bool,
}

impl SettingsPanel {
    pub fn new(ui_config: &lumino_core::storage::config::UiConfig) -> Self {
        Self {
            selected_menu_index: 0,
            synth_backend: ui_config.preferred_backend,
            soundfont_path: ui_config.soundfont_path.clone(),
            use_native_titlebar: ui_config.use_native_titlebar,
        }
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::MenuSelected(idx) => {
                self.selected_menu_index = idx;
            }
            Event::SynthBackendChanged(backend) => {
                self.synth_backend = backend;
            }
            Event::SoundfontPathChanged(path) => {
                self.soundfont_path = path;
            }
            Event::BrowseSoundfont => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("音色库文件", &["sf2", "sfz"])
                    .add_filter("SF2 文件", &["sf2"])
                    .add_filter("SFZ 文件", &["sfz"])
                    .add_filter("所有文件", &["*"])
                    .pick_file()
                {
                    self.soundfont_path = path.to_string_lossy().into_owned();
                }
            }
            Event::NativeTitlebarChanged(enabled) => {
                self.use_native_titlebar = enabled;
            }
        }
    }
}

/// 渲染设置面板主视图
pub fn view<'a>(settings: &SettingsPanel, window: &window::Window) -> Element<'a> {
    let menu_items = create_menu_items();

    let menu_list = render_menu_list(settings, window, &menu_items);
    let content_area = render_content_area(settings);

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
fn render_content_area<'a>(
    settings: &SettingsPanel,
) -> iced_widget::Container<'a, Message, Theme, crate::Renderer> {
    let content = match settings.selected_menu_index {
        0 => render_general_settings(),
        1 => render_audio_settings(settings),
        2 => render_ui_settings(settings),
        3 => render_shortcut_settings(),
        4 => render_about_settings(),
        _ => render_placeholder("设置内容区域"),
    };

    container(content)
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

fn render_placeholder<'a>(
    content: &'a str,
) -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    column![
        text("设置")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text(content)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
}

fn render_general_settings<'a>() -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    column![
        text("常规")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text("常规设置内容")
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
}

fn render_audio_settings<'a>(
    settings: &SettingsPanel,
) -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    let synth_options = [SynthBackend::XSynth, SynthBackend::Kdmapi];

    let mut col = column![
        text("音频")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 合成器后端选择
        row![
            text("合成器:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(synth_options, Some(settings.synth_backend), |backend| {
                Message::Settings(Event::SynthBackendChanged(backend))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
    ];

    // 只在 XSynth 模式下显示音色库选择
    if settings.synth_backend == SynthBackend::XSynth {
        col = col.push(
            row![
                text("音色库:")
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                iced_widget::space().width(SPACING_MAIN),
                text_input("选择音色库文件 (SFZ/SF2)...", &settings.soundfont_path)
                    .width(Length::Fill)
                    .on_input(|s| Message::Settings(Event::SoundfontPathChanged(s))),
            ]
            .spacing(SPACING_ICON_LABEL)
            .align_y(Alignment::Center),
        );
        col = col.push(iced_widget::space().height(SPACING_CONTENT));
        col = col.push(button("浏览...").on_press(Message::Settings(Event::BrowseSoundfont)));
        col = col.push(iced_widget::space().height(20));
        col = col.push(
            text("XSynth: 内置高性能合成器，支持SFZ/SF2格式音色库")
                .size(12.0)
                .style(create_placeholder_text_style()),
        );
        col = col.push(
            text("KDMAPI: 使用系统KDMAPI驱动，需要安装OmniMIDI")
                .size(12.0)
                .style(create_placeholder_text_style()),
        );
    } else {
        col = col.push(
            text("KDMAPI 模式使用系统驱动，无需音色库")
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        );
    }

    col.spacing(SPACING_CONTENT).padding(PADDING_CONTENT)
}

fn render_ui_settings<'a>(
    settings: &SettingsPanel,
) -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    // 创建复选框
    let native_titlebar_checkbox = iced_widget::Checkbox::new(settings.use_native_titlebar)
        .label("使用经典系统标题栏")
        .on_toggle(|enabled| Message::Settings(Event::NativeTitlebarChanged(enabled)));

    column![
        text("界面")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 使用经典系统标题栏选项
        row![native_titlebar_checkbox,]
            .spacing(SPACING_ICON_LABEL)
            .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("启用后，将使用系统原生标题栏，隐藏 Logo 和自定义窗口控制按钮")
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
}

fn render_shortcut_settings<'a>() -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    column![
        text("快捷键")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text("快捷键设置内容")
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
}

fn render_about_settings<'a>() -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    column![
        text("关于")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text("Lumino").size(16.0).style(create_content_text_style()),
        text("版本 1.0.0")
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(10),
        text("一个高效的MIDI编辑工具")
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
}
