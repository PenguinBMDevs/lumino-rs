//! 设置页面 - 界面设置

mod auto_scroll;
mod font;
mod interaction;
mod theme_cards;

use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text};
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::SettingsPanel;
use lumino_extras::i18n::{Language, settings_translations};

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

    // 字体设置部分
    let font_section = font::build_font_section(settings, t, system_fonts);

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
        // 主题选择（卡片预览：点击即切换，选中态高亮边框）
        text(t.theme)
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        theme_cards::view(settings.display.language, &window.theme.to_string()),
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
        auto_scroll::build_auto_scroll_section(settings, t),
        // 框选框模式设置
        interaction::build_interaction_section(settings, t),
        iced_widget::space().height(24),
        // ── 自动化曲线连线粗细 section ──
        row![
            text(t.ui_automation_line_thickness)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(200.0),
            iced_widget::slider(
                1.0..=10.0,
                settings.editing.automation_line_thickness,
                |v| { Message::Settings(crate::Event::AutomationLineThicknessChanged(v)) }
            )
            .step(0.5_f32)
            .width(200.0),
            text(format!(
                "{:.1} px",
                settings.editing.automation_line_thickness
            ))
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
            iced_widget::slider(
                50.0..=2000.0,
                settings.logging.monitor_refresh_interval_ms,
                |v| { Message::Settings(crate::Event::MonitorRefreshIntervalChanged(v)) }
            )
            .step(1.0_f32)
            .width(200.0),
            text(format!(
                "{:.0} ms",
                settings.logging.monitor_refresh_interval_ms
            ))
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
