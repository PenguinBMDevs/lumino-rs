use super::*;

use super::i2m_config::build_config_section;

/// 图片转 MIDI 面板内容（原 view 主体）
pub(super) fn i2m_panel<'a>(
    right_sidebar: &'a RightSidebar,
    window: &'a window::Window,
    _language: Language,
) -> Element<'a> {
    // 面板内"选择图片文件"按钮：标准 iced 按钮（无图标），居左放置
    let select_btn = button(iced_widget::text("选择图片文件").size(13))
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

    // 文件选择按钮居左，选中文件后在按钮右侧提示
    let mut select_row = Row::new().spacing(6).align_y(Alignment::Center);
    select_row = select_row.push(select_btn);
    if right_sidebar.selected_image_path.is_some() {
        select_row = select_row.push(iced_widget::text("选择了一个图片文件").size(12).style(
            |theme: &Theme| iced_widget::text::Style {
                color: Some(theme.extended_palette().background.strong.text),
            },
        ));
    }

    // 转换参数配置区（始终显示，便于预设参数）
    let config_section = build_config_section(right_sidebar);

    // 转换按钮：仅在选中文件后出现（标准 iced 按钮，无图标）
    let mut content_col = Column::new()
        .spacing(8)
        .padding(8)
        .width(Length::Fill)
        .push(panel_header("图片转 MIDI", window))
        .push(select_row)
        .push(config_section);
    if right_sidebar.selected_image_path.is_some() {
        let convert_btn = button(
            iced_widget::text(if right_sidebar.converting {
                "转换中..."
            } else {
                "转换为 MIDI"
            })
            .size(13),
        )
        .width(Length::Fill)
        .padding(6)
        .style(move |theme: &Theme, status| {
            let p = theme.extended_palette();
            let disabled = right_sidebar.converting;
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
        content_col = content_col.push(convert_btn);
    }

    let content = container(
        // 面板内容可能超出可视高度（参数区），包滚动容器
        iced_widget::scrollable(content_col).height(Length::Fill),
    )
    .width(Length::Fixed(
        right_sidebar.panel_width - RESIZE_HANDLE_WIDTH,
    ))
    .height(Length::Fill)
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style::default().background(palette.background.weakest.color)
    });

    content.into()
}

/// 面板标题文本（跟随主题：暗色白、亮色黑）
fn panel_header<'a>(title: &'a str, _window: &'a window::Window) -> Element<'a> {
    iced_widget::text(title)
        .size(14)
        .style(|theme: &Theme| iced_widget::text::Style {
            color: Some(theme.extended_palette().background.neutral.text),
        })
        .into()
}
