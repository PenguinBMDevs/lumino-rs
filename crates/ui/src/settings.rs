use iced_core::{Alignment, Border, Length, Padding};
use iced_widget::{button, column, container, row, text};

use crate::{Element, Message, Theme, resources::icon::{self, Icon}, window};

#[derive(Debug, Clone)]
pub enum Event {
    MenuSelected(usize),
}

#[derive(Debug, Clone)]
pub struct SettingsPanel {
    pub selected_menu: usize,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            selected_menu: 0,
        }
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::MenuSelected(idx) => {
                self.selected_menu = idx;
            }
        }
    }
}

pub fn view<'a>(settings: &SettingsPanel, window: &window::Window) -> Element<'a> {
    let menu_items = vec![
        ("常规", Icon::Gear),
        ("音频", Icon::WaveForm),
        ("界面", Icon::FolderTree),
        ("快捷键", Icon::Clock),
        ("关于", Icon::GitHub),
    ];

    let menu_list = {
        let mut col = column![].spacing(0).padding(1);

        for (idx, (label, icon)) in menu_items.iter().enumerate() {
            let is_selected = idx == settings.selected_menu;

            let icon_el = icon::view_with_size_and_theme(*icon, 18, 18, Some(&window.theme));

            let label_text = text(*label)
                .size(14)
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

            let arrow = text(">")
                .size(12)
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(palette.background.weak.text),
                    }
                });

            let item_row = row![
                container(icon_el).width(24).align_x(Alignment::Center),
                label_text,
                arrow,
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding(Padding::new(12.0).left(16.0).right(16.0));

            let item_btn = button(item_row)
                .width(Length::Fill)
                .on_press(Message::Settings(Event::MenuSelected(idx)))
                .style(move |theme: &Theme, status| {
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
                });

            col = col.push(item_btn);
        }

        container(col)
            .width(200)
            .height(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(iced_core::Background::Color(palette.background.weak.color)),
                    border: Border::default()
                        .rounded(16.0)
                        .width(1.0)
                        .color(palette.background.strong.color),
                    shadow: iced_core::Shadow {
                        color: iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.15),
                        offset: iced_core::Vector::new(0.0, 4.0),
                        blur_radius: 8.0,
                    },
                    text_color: Some(palette.background.base.text),
                    snap: false,
                }
            })
    };

    let content_area = container(
        column![
            text("设置")
                .size(18)
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(palette.background.base.text),
                    }
                }),
            iced_widget::space().height(20),
            text("设置内容区域")
                .size(14)
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(palette.background.weak.text),
                    }
                }),
        ]
        .spacing(10)
        .padding(20),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(iced_core::Background::Color(palette.background.base.color)),
            border: Border::default()
                .rounded(21.0)
                .width(1.0)
                .color(palette.background.strong.color),
            shadow: iced_core::Shadow {
                color: iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                offset: iced_core::Vector::new(0.0, 4.0),
                blur_radius: 4.0,
            },
            text_color: Some(palette.background.base.text),
            snap: false,
        }
    });

    let main_content = row![
        menu_list,
        iced_widget::space().width(16),
        content_area,
    ]
    .spacing(0)
    .padding(20);

    container(main_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(iced_core::Background::Color(
                    palette.background.weakest.color
                )),
                text_color: Some(palette.background.base.text),
                snap: false,
                ..Default::default()
            }
        })
        .into()
}
