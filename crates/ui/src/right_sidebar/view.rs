//! 右侧栏视图渲染

use iced_core::{Alignment, Color, Length};
use iced_widget::{Column, Row, Space, button, container, mouse_area, tooltip};
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::RightSidebarAction;

use crate::resources::icon::{self, Icon};
use crate::right_sidebar::core::{RESIZE_HANDLE_WIDTH, ROUTE_BAR_WIDTH, RightSidebar};
use crate::widget;
use crate::{Element, Message, Theme, window};

/// 渲染右侧栏视图（图标按钮列 + 向左展开的面板）
pub fn view<'a>(
    right_sidebar: &'a RightSidebar,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let t = main_translations(language);

    // 图标列（垂直排列按钮）
    let col = Column::new()
        .spacing(2)
        .width(ROUTE_BAR_WIDTH)
        .height(Length::Fill)
        // 图片转 MIDI 按钮：始终可见，点击自动展开/收起面板并亮灯
        .push(sidebar_button(
            Icon::ImageToMidi,
            t.tool_image_to_midi,
            Message::RightSidebar(RightSidebarAction::ImageToMidiClicked),
            right_sidebar.panel_visible,
            window,
        ))
        .push(Space::new().height(Length::Fill));

    // 图标列容器
    let route_bar = container(col)
        .width(ROUTE_BAR_WIDTH)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weaker.color)
        });

    // 如果面板可见，渲染内容面板 + 调整手柄
    if right_sidebar.panel_visible {
        // 面板内"选择图片文件"按钮
        let select_btn = button(
            Row::new()
                .push(icon::view_with_size_and_theme(
                    Icon::ImageToMidi,
                    16,
                    16,
                    Some(&window.theme),
                ))
                .push(iced_widget::text("选择图片文件").size(13))
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(6)
        .style(move |theme: &Theme, status| {
            let p = theme.extended_palette();
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => p.background.base.color,
                _ => p.background.weak.color,
            };
            button::Style {
                text_color: p.background.base.text,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        })
        .on_press(Message::RightSidebar(RightSidebarAction::SelectImageFile));

        // 面板内容：文件选择按钮 + 已选图片路径标注 + 转换按钮
        let file_info: Element<'a> = if let Some(path) = &right_sidebar.selected_image_path {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            iced_widget::column![
                iced_widget::text(format!("文件: {name}")).size(13),
                // 路径文字用横向滚动容器包裹，避免超出面板区域
                iced_widget::scrollable(
                    iced_widget::text(format!("路径: {}", path.display()))
                        .size(11)
                        .style(|theme: &Theme| iced_widget::text::Style {
                            color: Some(theme.extended_palette().background.strong.text),
                        }),
                )
                .direction(iced_widget::scrollable::Direction::Horizontal(
                    iced_widget::scrollable::Scrollbar::new()
                        .width(4)
                        .scroller_width(4),
                ))
                .height(Length::Shrink),
            ]
            .spacing(4)
            .into()
        } else {
            iced_widget::text("尚未选择图片文件")
                .size(12)
                .style(|theme: &Theme| iced_widget::text::Style {
                    color: Some(theme.extended_palette().background.strong.text),
                })
                .into()
        };

        // 转换按钮：有图片时可用；转换中显示状态文本
        let convert_btn = button(
            Row::new()
                .push(icon::view_with_size_and_theme(
                    Icon::ImageToMidi,
                    16,
                    16,
                    Some(&window.theme),
                ))
                .push(
                    iced_widget::text(if right_sidebar.converting {
                        "转换中..."
                    } else {
                        "转换为 MIDI"
                    })
                    .size(13),
                )
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(6)
        .style(move |theme: &Theme, status| {
            let p = theme.extended_palette();
            let disabled = right_sidebar.selected_image_path.is_none() || right_sidebar.converting;
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed if !disabled => {
                    p.primary.base.color
                }
                _ => p.background.weak.color,
            };
            let text_color = if disabled {
                p.background.strong.text
            } else {
                p.background.base.text
            };
            button::Style {
                text_color,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        })
        .on_press(Message::RightSidebar(RightSidebarAction::ConvertClicked));

        let content = container(
            Column::new()
                .spacing(8)
                .padding(8)
                .push(panel_header("图片转 MIDI", window))
                .push(select_btn)
                .push(file_info)
                .push(convert_btn),
        )
        .width(Length::Fixed(
            right_sidebar.panel_width - RESIZE_HANDLE_WIDTH,
        ))
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weakest.color)
        });

        // 调整大小手柄（放在面板右侧，紧贴图标列）
        let resize_handle = mouse_area(
            container(
                Space::new()
                    .width(Length::Fixed(RESIZE_HANDLE_WIDTH))
                    .height(Length::Fill),
            )
            .style(move |_theme: &Theme| {
                let bg = if right_sidebar.is_resizing {
                    palette.primary.strong.color
                } else {
                    palette.background.weakest.color
                };
                container::Style::default().background(bg)
            }),
        )
        .interaction(iced_core::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::RightSidebar(RightSidebarAction::ResizeDragStarted))
        .on_release(Message::RightSidebar(RightSidebarAction::ResizeDragEnded));

        // 面板内容 + 调整手柄（手柄在面板右侧）
        let panel_with_handle = Row::new().push(content).push(resize_handle);
        let panel_container = container(panel_with_handle)
            .width(Length::Fixed(right_sidebar.panel_width))
            .height(Length::Fill);

        // 顺序：面板内容（向左展开）→ 图标列（固定在右侧）
        Row::new()
            .push(panel_container)
            .push(route_bar)
            .height(Length::Fill)
            .into()
    } else {
        // 面板不可见，只显示图标列
        route_bar.into()
    }
}

/// 面板标题文本（无装饰小条）
fn panel_header<'a>(title: &'a str, _window: &'a window::Window) -> Element<'a> {
    iced_widget::text(title).size(14).into()
}

/// 与左侧栏统一的按钮样式：48x48，右侧2px指示条（激活时亮灯），图标+间距12px
fn sidebar_button<'a>(
    icon_enum: Icon,
    tooltip_text: &'a str,
    on_press: Message,
    active: bool,
    window: &'a window::Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    // 右侧2px指示条：激活时亮灯（与左侧栏同理，但位置在右侧），否则透明
    let split =
        container(Space::new())
            .width(2)
            .height(Length::Fill)
            .style(move |_theme: &Theme| {
                let background = if active {
                    palette.primary.base.color
                } else {
                    Color::TRANSPARENT
                };
                container::Style::default().background(background)
            });

    let icon_img = icon::view_with_size_and_theme(icon_enum, 20, 20, Some(&window.theme));

    // 图标容器占满除去右侧指示条外的宽度，使图标在按钮内水平居中
    let icon_holder = container(icon_img)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    // 镜像布局：图标居中，指示条固定在右侧
    let inner = Row::new()
        .push(icon_holder)
        .push(split)
        .width(Length::Fill)
        .height(Length::Fill);

    let btn = button(inner)
        .width(48)
        .height(48)
        .padding(0)
        .style(move |theme: &Theme, status| {
            use button::Status::*;
            let p = theme.extended_palette();
            let text_color = match status {
                Hovered | Pressed => p.background.base.color,
                _ => p.background.weakest.color,
            };
            button::Style {
                text_color,
                ..Default::default()
            }
            .with_background(Color::TRANSPARENT)
        })
        .on_press(on_press);

    widget::with_tooltip(btn, tooltip_text, tooltip::Position::Left).into()
}
