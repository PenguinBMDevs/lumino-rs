//! 设置面板模块
//!
//! 该模块已拆分为以下子模块：
//! - `pages`: 各设置页面（常规、音频、界面、快捷键、关于）
//! - `components`: 可复用组件（样式、常量）

pub mod components;
pub mod pages;

use iced_core::{Alignment, Border, Length, Padding};
use iced_widget::{button, column, container, row, text};

use crate::{
    Element, Message, Theme,
    resources::icon::{self, Icon},
    window,
};
use lumino_core::storage::config::SynthBackend;

use components::*;
use pages::*;

#[derive(Debug, Clone)]
pub enum Event {
    MenuSelected(usize),
    SynthBackendChanged(SynthBackend),
    SoundfontPathChanged(String),
    BrowseSoundfont,
    NativeTitlebarChanged(bool),
    XSynthBufferChanged(f64),
    XSynthSampleRateChanged(u32),
    XSynthThreadsChanged(i32),
    XSynthFadeOutChanged(bool),
    ThemeChanged(String),
}

#[derive(Debug, Clone)]
pub struct SettingsPanel {
    pub selected_menu_index: usize,
    pub synth_backend: SynthBackend,
    pub soundfont_path: String,
    pub use_native_titlebar: bool,
    pub xsynth_buffer_ms: f64,
    pub xsynth_sample_rate: u32,
    pub xsynth_threads: i32,
    pub xsynth_fade_out: bool,
}

impl SettingsPanel {
    pub fn new(ui_config: &lumino_core::storage::config::UiConfig) -> Self {
        Self {
            selected_menu_index: 0,
            synth_backend: ui_config.preferred_backend,
            soundfont_path: ui_config.soundfont_path.clone(),
            use_native_titlebar: ui_config.use_native_titlebar,
            xsynth_buffer_ms: ui_config.xsynth_buffer_ms,
            xsynth_sample_rate: ui_config.xsynth_sample_rate,
            xsynth_threads: ui_config.xsynth_threads,
            xsynth_fade_out: ui_config.xsynth_fade_out_killing,
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
            Event::XSynthBufferChanged(ms) => {
                self.xsynth_buffer_ms = ms;
            }
            Event::XSynthSampleRateChanged(sr) => {
                self.xsynth_sample_rate = sr;
            }
            Event::XSynthThreadsChanged(t) => {
                self.xsynth_threads = t;
            }
            Event::XSynthFadeOutChanged(f) => {
                self.xsynth_fade_out = f;
            }
            Event::ThemeChanged(_) => {
                // 主题变更由外部处理
            }
        }
    }
}

/// 渲染设置面板主视图
pub fn view<'a>(settings: &SettingsPanel, window: &window::Window) -> Element<'a> {
    let menu_items = create_menu_items();

    let menu_list = render_menu_list(settings, window, &menu_items);
    let content_area = render_content_area(settings, window);

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

fn create_menu_items() -> Vec<(&'static str, Icon)> {
    vec![
        ("常规", Icon::Gear),
        ("音频", Icon::WaveForm),
        ("界面", Icon::FolderTree),
        ("快捷键", Icon::Clock),
        ("关于", Icon::GitHub),
    ]
}

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

fn render_content_area<'a>(
    settings: &SettingsPanel,
    window: &window::Window,
) -> iced_widget::Container<'a, Message, Theme, crate::Renderer> {
    let content = match settings.selected_menu_index {
        0 => general_view(),
        1 => audio_view(settings),
        2 => ui_settings_view(settings, window),
        3 => shortcuts_view(),
        4 => about_view(),
        _ => render_placeholder("设置内容区域").into(),
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(create_content_container_style())
}

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
