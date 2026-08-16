//! 设置页面 - 界面设置

use iced_core::Alignment;
use iced_widget::{button, column, pick_list, row, text, text_input};
use lumino_ui_core::{Element, Message, Theme};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::SettingsPanel;
use lumino_core::storage::config::SelectionBoxMode;
use lumino_extras::i18n::{Language, SettingsTranslations, settings_translations};
use lumino_ui_core::window;

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
            name: lumino_extras::i18n::selection_box_mode_name(mode, lang),
        }
    }
}

/// 渲染界面设置页面
pub fn view<'a>(
    settings: &'a SettingsPanel,
    window: &lumino_ui_core::window::Window,
    system_fonts: &[lumino_note_core::font_scanner::FontInfo],
) -> Element<'a> {
    let t = settings_translations(settings.display.language);

    // 创建复选框
    let native_titlebar_checkbox = if cfg!(target_os = "macos") {
        row![] // macOS 不需要这个选项
    } else {
        row![
            iced_widget::Checkbox::new(settings.synth.use_native_titlebar)
                .label(t.native_titlebar)
                .on_toggle(|enabled| {
                    Message::Settings(crate::Event::NativeTitlebarChanged(enabled))
                })
        ]
    };

    // 主题选项（在 Iced 内置主题前插入高对比度选项）
    let hc_canonical = lumino_ui_core::theme::HIGH_CONTRAST_DISPLAY;
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

    // 字体设置部分
    let font_section = build_font_section(settings, t, system_fonts);

    // 语言选项
    let language_options = Language::all();
    let current_language = settings.display.language;

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
                Message::Settings(crate::Event::LanguageChanged(lang))
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
            iced_widget::Checkbox::new(settings.display.icon_hidpi)
                .label(t.hidpi_icon)
                .on_toggle(|enabled| {
                    Message::Settings(crate::Event::IconHiDPIChanged(enabled))
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
        build_auto_scroll_section(settings, t),
        // 框选框模式设置
        build_interaction_section(settings, t),
        iced_widget::space().height(24),
        // ── 自动化曲线连线粗细 section ──
        row![
            text(t.ui_automation_line_thickness)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(200.0),
            iced_widget::slider(1.0..=10.0, settings.editing.automation_line_thickness, |v| {
                Message::Settings(crate::Event::AutomationLineThicknessChanged(v))
            })
            .step(0.5_f32)
            .width(200.0),
            text(format!("{:.1} px", settings.editing.automation_line_thickness))
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(50.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.ui_automation_line_thickness_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // ── 底边栏监控数据刷新间隔 ──
        row![
            text(t.ui_monitor_refresh_interval)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(200.0),
            iced_widget::slider(50.0..=2000.0, settings.logging.monitor_refresh_interval_ms, |v| {
                Message::Settings(crate::Event::MonitorRefreshIntervalChanged(v))
            })
            .step(1.0_f32)
            .width(200.0),
            text(format!("{:.0} ms", settings.logging.monitor_refresh_interval_ms))
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(60.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.ui_monitor_refresh_interval_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}

/// 构建字体设置部分（系统字体下拉、自定义路径、浏览按钮、说明）
fn build_font_section<'a>(
    settings: &'a SettingsPanel,
    t: &'static SettingsTranslations,
    system_fonts: &[lumino_note_core::font_scanner::FontInfo],
) -> Element<'a> {
    // 字体选项 - 从系统扫描的字体列表构建
    let font_options: Vec<String> = system_fonts.iter().map(|f| f.name.clone()).collect();

    // 字体选择下拉菜单
    let font_dropdown = pick_list(
        font_options,
        Some(settings.editing.program_font_name.clone()).filter(|s| !s.is_empty()),
        |font_name| Message::Settings(crate::Event::ProgramFontNameChanged(font_name)),
    )
    .width(200.0)
    .placeholder("选择系统字体...");

    // 自定义字体路径输入
    let font_path_input = text_input(t.font_path_placeholder, &settings.editing.program_font_path)
        .on_input(|path| Message::Settings(crate::Event::ProgramFontPathChanged(path)));

    // 浏览字体文件按钮
    let browse_font_button =
        button(t.browse).on_press(Message::Settings(crate::Event::BrowseProgramFont));

    column![
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
    ]
    .into()
}

/// 构建自动滚动配置部分
fn build_auto_scroll_section<'a>(
    settings: &'a SettingsPanel,
    t: &'static SettingsTranslations,
) -> Element<'a> {
    column![
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
            text_input(t.pixel, &settings.auto_scroll.fixed_position.to_string())
                .on_input(|v| Message::Settings(crate::Event::AutoScrollFixedPositionChanged(v)))
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
                &settings.auto_scroll.page_trigger_offset.to_string()
            )
            .on_input(|v| Message::Settings(crate::Event::AutoScrollPageTriggerOffsetChanged(v)))
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
                &settings.auto_scroll.page_return_position.to_string()
            )
            .on_input(|v| Message::Settings(crate::Event::AutoScrollPageReturnPositionChanged(v)))
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
    ]
    .spacing(SPACING_CONTENT)
    .into()
}

/// 构建交互设置部分（框选框模式、256 键钢琴卷帘、力度面板样式、播放键盘颜色）
fn build_interaction_section<'a>(
    settings: &'a SettingsPanel,
    t: &'static SettingsTranslations,
) -> Element<'a> {
    column![
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
                    LocalizedSelectionBox::new(SelectionBoxMode::Direct, settings.display.language),
                    LocalizedSelectionBox::new(SelectionBoxMode::Spring, settings.display.language),
                ],
                Some(LocalizedSelectionBox::new(
                    settings.editing.selection_box_mode,
                    settings.display.language
                )),
                |ls| Message::Settings(crate::Event::SelectionBoxModeChanged(ls.inner)),
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
            iced_widget::Checkbox::new(settings.display.enable_256key)
                .label(t.enable_256key)
                .on_toggle(|enabled| {
                    Message::Settings(crate::Event::Enable256keyChanged(enabled))
                }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text(t.enable_256key_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 力度面板显示样式（曲线/柱状图切换）
        row![
            iced_widget::Checkbox::new(settings.display.velocity_curve_style)
                .label("力度面板曲线显示（默认）")
                .on_toggle(|enabled| {
                    Message::Settings(crate::Event::VelocityCurveStyleChanged(enabled))
                }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("关闭后使用柱状图显示力度值")
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 播放键盘颜色指示
        row![
            iced_widget::Checkbox::new(settings.display.playback_key_colors_enabled)
                .label("播放时键盘颜色指示（默认关闭）")
                .on_toggle(|enabled| {
                    Message::Settings(crate::Event::PlaybackKeyColorsEnabledChanged(enabled))
                }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("开启后播放时在钢琴键盘上高亮当前音符位置，占用额外内存")
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .into()
}
