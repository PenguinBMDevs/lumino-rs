use super::*;

/// 转换参数配置区：key 范围、目标高度、每像素 tick、颜色数、调色板算法
pub(super) fn build_config_section<'a>(right_sidebar: &'a RightSidebar) -> Element<'a> {
    let cfg = &right_sidebar.config;

    // 调色板算法下拉（选项为中文名，选中后回传索引）
    let palette_names: Vec<&'static str> =
        PALETTE_ALGORITHMS.iter().map(|(name, _)| *name).collect();
    let palette_current = palette_names
        .get(cfg.palette_index)
        .copied()
        .unwrap_or(palette_names[0]);
    let palette_control = iced_widget::pick_list(
        palette_names,
        Some(palette_current),
        |name: &'static str| {
            let idx = PALETTE_ALGORITHMS
                .iter()
                .position(|(n, _)| *n == name)
                .unwrap_or(0);
            Message::RightSidebar(RightSidebarAction::I2mPaletteChanged(idx))
        },
    )
    .text_size(12)
    .padding([3, 6])
    .width(Length::Fixed(118.0));

    Column::new()
        .spacing(4)
        .push(section_label("转换参数"))
        .push(config_row(
            "Key 范围",
            Row::new()
                .push(config_input(&cfg.start_key_text, I2mConfigField::StartKey))
                .push(iced_widget::text("~").size(12).style(|theme: &Theme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.strong.text),
                    }
                }))
                .push(config_input(&cfg.end_key_text, I2mConfigField::EndKey))
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
        ))
        .push(config_row(
            "目标高度",
            config_input(&cfg.target_height_text, I2mConfigField::TargetHeight),
        ))
        .push(config_row(
            "每像素 tick",
            config_input(&cfg.ticks_per_pixel_text, I2mConfigField::TicksPerPixel),
        ))
        .push(config_row(
            "颜色数",
            config_input(&cfg.color_count_text, I2mConfigField::ColorCount),
        ))
        .push(config_row("调色板", palette_control.into()))
        .into()
}

/// 小节标题（跟随主题：暗色白、亮色黑，与项目内面板标题一致）
fn section_label<'a>(title: &'a str) -> Element<'a> {
    iced_widget::text(title)
        .size(12)
        .style(|theme: &Theme| iced_widget::text::Style {
            color: Some(theme.extended_palette().background.neutral.text),
        })
        .into()
}

/// 单行配置：标签居左（文字色跟随主题），控件居右
fn config_row<'a>(label: &'a str, control: Element<'a>) -> Element<'a> {
    Row::new()
        .push(
            iced_widget::text(label)
                .size(12)
                .style(|theme: &Theme| iced_widget::text::Style {
                    color: Some(theme.extended_palette().background.strong.text),
                }),
        )
        .push(Space::new().width(Length::Fill))
        .push(control)
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}

/// 小型数字输入框（带边框，仅接受数字）
fn config_input<'a>(value: &'a str, field: I2mConfigField) -> Element<'a> {
    container(
        iced_widget::text_input("", value)
            .on_input(move |text| {
                Message::RightSidebar(RightSidebarAction::I2mConfigTextChanged { field, text })
            })
            .padding([3, 6])
            .size(iced_core::Pixels(12.0))
            .width(Length::Fixed(42.0)),
    )
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(palette.background.weak.color.into()),
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: palette.background.strong.color,
            },
            ..Default::default()
        }
    })
    .into()
}
