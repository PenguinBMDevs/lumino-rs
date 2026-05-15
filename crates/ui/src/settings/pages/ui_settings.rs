//! 设置页面 - 界面设置

use crate::{Element, Message, Theme};
use iced_core::Alignment;
use iced_widget::{button, column, pick_list, row, text, text_input};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::settings::SettingsPanel;
use crate::window;

/// 渲染界面设置页面
pub fn view<'a>(
    settings: &SettingsPanel,
    window: &crate::window::Window,
    system_fonts: &[lumino_core::font_scanner::FontInfo],
) -> Element<'a> {
    // 创建复选框
    let native_titlebar_checkbox = if cfg!(target_os = "macos") {
        row![] // macOS 不需要这个选项
    } else {
        row![
            iced_widget::Checkbox::new(settings.use_native_titlebar)
                .label("使用经典系统标题栏")
                .on_toggle(|enabled| {
                    Message::Settings(crate::settings::Event::NativeTitlebarChanged(enabled))
                })
        ]
    };

    // 主题选项
    let theme_options: Vec<String> = Theme::ALL.iter().map(|t| t.to_string()).collect();
    let current_theme = window.theme.to_string();

    // 字体选项 - 从系统扫描的字体列表构建
    let font_options: Vec<String> = system_fonts.iter().map(|f| f.name.clone()).collect();

    // 字体选择下拉菜单
    let font_dropdown = pick_list(
        font_options,
        Some(settings.program_font_name.clone()).filter(|s| !s.is_empty()),
        |font_name| Message::Settings(crate::settings::Event::ProgramFontNameChanged(font_name)),
    )
    .width(200.0)
    .placeholder("选择系统字体...");

    // 自定义字体路径输入
    let font_path_input = text_input("或输入字体文件路径...", &settings.program_font_path)
        .on_input(|path| Message::Settings(crate::settings::Event::ProgramFontPathChanged(path)));

    // 浏览字体文件按钮
    let browse_font_button =
        button("浏览...").on_press(Message::Settings(crate::settings::Event::BrowseProgramFont));

    // 字体设置部分
    let font_section = column![
        // 系统字体下拉菜单
        row![
            text("程序字体:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            font_dropdown,
            iced_widget::space().width(SPACING_ICON_LABEL),
            text("或").size(12.0).style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_ICON_LABEL),
        // 自定义路径输入
        row![
            font_path_input.width(250.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            browse_font_button,
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 字体设置说明
        text("选择系统字体或指定自定义字体文件路径。如果自定义路径无效，将回退到默认字体。")
            .size(12.0)
            .style(create_placeholder_text_style()),
    ];

    column![
        text("界面")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 主题选择
        row![
            text("主题:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(theme_options, Some(current_theme), |theme| {
                Message::Window(window::Event::Theme(theme))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 字体设置
        font_section,
        // 使用经典系统标题栏选项
        row![native_titlebar_checkbox,]
            .spacing(SPACING_ICON_LABEL)
            .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("启用后，将使用系统原生标题栏，隐藏 Logo 和自定义窗口控制按钮")
            .size(12.0)
            .style(create_placeholder_text_style()),
        // HiDPI 图标渲染选项
        row![
            iced_widget::Checkbox::new(settings.icon_hidpi)
                .label("启用 HiDPI 图标渲染（推荐）")
                .on_toggle(|enabled| {
                    Message::Settings(crate::settings::Event::IconHiDPIChanged(enabled))
                })
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("开启后图标以2x分辨率渲染，在视网膜屏幕上更清晰。关闭可节省少量内存和渲染开销。")
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // 自动滚动配置
        text("自动滚动设置")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(12),
        // 模式1：指示线固定位置
        row![
            text("模式1 - 指示线固定位置:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("像素", &settings.auto_scroll_fixed_position.to_string())
                .on_input(|v| Message::Settings(
                    crate::settings::Event::AutoScrollFixedPositionChanged(v)
                ))
                .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text("像素 (从左边缘算起)")
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 模式2：翻页触发位置
        row![
            text("模式2 - 翻页触发位置:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(
                "像素",
                &settings.auto_scroll_page_trigger_offset.to_string()
            )
            .on_input(|v| Message::Settings(
                crate::settings::Event::AutoScrollPageTriggerOffsetChanged(v)
            ))
            .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text("像素 (从右边缘算起)")
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 模式2：翻页后回到的位置
        row![
            text("模式2 - 翻页后位置:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(
                "像素",
                &settings.auto_scroll_page_return_position.to_string()
            )
            .on_input(|v| Message::Settings(
                crate::settings::Event::AutoScrollPageReturnPositionChanged(v)
            ))
            .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text("像素 (从左边缘算起)")
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("设置卷帘自动滚动时演奏指示线的位置行为")
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // 256键钢琴卷帘设置
        text("钢琴卷帘")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(12),
        row![
            iced_widget::Checkbox::new(settings.enable_256key)
                .label("启用 256 键扩展钢琴卷帘")
                .on_toggle(|enabled| {
                    Message::Settings(crate::settings::Event::Enable256keyChanged(enabled))
                }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("开启后钢琴卷帘拓展至 256 键 (0-255)，扩展区域（128-255）颜色略深以便区分。需要较强的 GPU 性能。")
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
