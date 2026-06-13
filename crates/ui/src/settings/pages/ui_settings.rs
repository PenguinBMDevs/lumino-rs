//! 设置页面 - 界面设置

use crate::{Element, Message, Theme};
use iced_core::Alignment;
use iced_widget::{button, column, pick_list, row, text, text_input};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::settings::SettingsPanel;
use crate::window;
use lumino_core::i18n::{Language, settings_translations};
use lumino_core::storage::config::SelectionBoxMode;

/// 本地化主题选项（显示名 vs 规范标识符）
#[derive(Debug, Clone)]
struct ThemeOption {
    display: String,
    value: String,
}

impl std::fmt::Display for ThemeOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display)
    }
}

impl PartialEq for ThemeOption {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for ThemeOption {}

/// 本地化框选框模式包装
#[derive(Debug, Clone, Copy)]
struct LocalizedSelectionBox {
    inner: SelectionBoxMode,
    name: &'static str,
}

impl PartialEq for LocalizedSelectionBox {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedSelectionBox {}

impl std::fmt::Display for LocalizedSelectionBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedSelectionBox {
    fn new(mode: SelectionBoxMode, lang: Language) -> Self {
        Self {
            inner: mode,
            name: lumino_core::i18n::selection_box_mode_name(mode, lang),
        }
    }
}

/// 渲染界面设置页面
pub fn view<'a>(
    settings: &SettingsPanel,
    window: &crate::window::Window,
    system_fonts: &[lumino_core::font_scanner::FontInfo],
) -> Element<'a> {
    let t = settings_translations(settings.language);

    // 创建复选框
    let native_titlebar_checkbox = if cfg!(target_os = "macos") {
        row![] // macOS 不需要这个选项
    } else {
        row![
            iced_widget::Checkbox::new(settings.use_native_titlebar)
                .label(t.native_titlebar)
                .on_toggle(|enabled| {
                    Message::Settings(crate::settings::Event::NativeTitlebarChanged(enabled))
                })
        ]
    };

    // 主题选项（在 Iced 内置主题前插入高对比度选项）
    let hc_canonical = crate::theme::HIGH_CONTRAST_DISPLAY;
    let mut theme_options: Vec<ThemeOption> = vec![ThemeOption {
        display: t.high_contrast.to_string(),
        value: hc_canonical.to_string(),
    }];
    theme_options.extend(Theme::ALL.iter().map(|t| ThemeOption {
        display: t.to_string(),
        value: t.to_string(),
    }));
    let current_theme = ThemeOption {
        display: window.theme.to_string(),
        value: window.theme.to_string(),
    };

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
    let font_path_input = text_input(t.font_path_placeholder, &settings.program_font_path)
        .on_input(|path| Message::Settings(crate::settings::Event::ProgramFontPathChanged(path)));

    // 浏览字体文件按钮
    let browse_font_button =
        button(t.browse).on_press(Message::Settings(crate::settings::Event::BrowseProgramFont));

    // 字体设置部分
    let font_section = column![
        // 系统字体下拉菜单
        row![
            text(t.program_font)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            font_dropdown,
            iced_widget::space().width(SPACING_ICON_LABEL),
            text(t.or).size(12.0).style(create_placeholder_text_style()),
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
        text(t.font_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    ];

    // 语言选项
    let language_options = Language::all();
    let current_language = settings.language;

    column![
        text(t.ui_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 语言选择
        row![
            text("语言 / Language:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(language_options, Some(current_language), |lang| {
                Message::Settings(crate::settings::Event::LanguageChanged(lang))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 主题选择
        row![
            text(t.theme)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(theme_options, Some(current_theme), |to| {
                Message::Window(window::Event::Theme(to.value))
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
        text(t.native_titlebar_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        // HiDPI 图标渲染选项
        row![
            iced_widget::Checkbox::new(settings.icon_hidpi)
                .label(t.hidpi_icon)
                .on_toggle(|enabled| {
                    Message::Settings(crate::settings::Event::IconHiDPIChanged(enabled))
                })
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text(t.hidpi_icon_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // 自动滚动配置
        text(t.auto_scroll)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(12),
        // 模式1：指示线固定位置
        row![
            text(t.auto_scroll_fixed)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(t.pixel, &settings.auto_scroll_fixed_position.to_string())
                .on_input(|v| Message::Settings(
                    crate::settings::Event::AutoScrollFixedPositionChanged(v)
                ))
                .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text(t.from_left)
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 模式2：翻页触发位置
        row![
            text(t.auto_scroll_trigger)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(
                t.pixel,
                &settings.auto_scroll_page_trigger_offset.to_string()
            )
            .on_input(|v| Message::Settings(
                crate::settings::Event::AutoScrollPageTriggerOffsetChanged(v)
            ))
            .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text(t.from_right)
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 模式2：翻页后回到的位置
        row![
            text(t.auto_scroll_return)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(
                t.pixel,
                &settings.auto_scroll_page_return_position.to_string()
            )
            .on_input(|v| Message::Settings(
                crate::settings::Event::AutoScrollPageReturnPositionChanged(v)
            ))
            .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text(t.from_left)
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text(t.auto_scroll_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // 框选框模式设置
        text(t.interaction)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(12),
        row![
            text(t.selection_box_mode)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(
                vec![
                    LocalizedSelectionBox::new(SelectionBoxMode::Direct, settings.language),
                    LocalizedSelectionBox::new(SelectionBoxMode::Spring, settings.language),
                ],
                Some(LocalizedSelectionBox::new(settings.selection_box_mode, settings.language)),
                |ls| Message::Settings(crate::settings::Event::SelectionBoxModeChanged(ls.inner)),
            )
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text(t.selection_box_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // 256键钢琴卷帘设置
        text(t.piano_roll)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(12),
        row![
            iced_widget::Checkbox::new(settings.enable_256key)
                .label(t.enable_256key)
                .on_toggle(|enabled| {
                    Message::Settings(crate::settings::Event::Enable256keyChanged(enabled))
                }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text(t.enable_256key_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 钢琴仿真键盘开关
        row![
            iced_widget::Checkbox::new(settings.use_textured_keyboard)
                .label(t.textured_keyboard)
                .on_toggle(|enabled| {
                    Message::Settings(crate::settings::Event::TexturedKeyboardChanged(enabled))
                }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text(t.textured_keyboard_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
