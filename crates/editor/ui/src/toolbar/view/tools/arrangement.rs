use super::*;

impl Toolbar {
    /// 渲染工程走带视图专用的工具选择区域
    ///
    /// 仅开启 yinhe 工程走带面板支持的工具：选择/铅笔/曲线/切割/橡皮擦。
    pub(super) fn render_arrangement_tools_section<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        _has_selection: bool,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        container(
            row![
                tool_selector(
                    icon::MousePointer,
                    t.tool_pointer,
                    Tool::Pointer,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Pointer))),
                ),
                space().width(4),
                tool_selector(
                    icon::Curve,
                    t.tool_curve,
                    Tool::Curve,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Curve))),
                ),
                space().width(4),
                tool_selector(
                    icon::Eraser,
                    t.tool_eraser,
                    Tool::Eraser,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Eraser))),
                ),
                space().width(4),
                flip_button(
                    icon::Speed,
                    t.tool_speed,
                    Event::speed_change(),
                    true,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Speed))),
                ),
            ]
            .align_y(Alignment::Center),
        )
        .width(iced_widget::core::Length::Shrink)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        })
        .into()
    }
}
