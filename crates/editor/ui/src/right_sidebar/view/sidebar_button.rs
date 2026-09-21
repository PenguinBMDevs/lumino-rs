use super::*;

/// 与左侧栏统一的按钮样式：48x48，右侧2px指示条（激活时亮灯），图标+间距12px
pub(super) fn sidebar_button<'a>(
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
