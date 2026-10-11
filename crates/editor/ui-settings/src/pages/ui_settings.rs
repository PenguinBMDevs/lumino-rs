//! 设置页面 - 界面设置

mod auto_scroll;
mod font;
mod interaction;
mod theme_cards;

use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text};
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::create_content_text_style;
use super::super::components::{with_setting_tooltip_inline, with_setting_tooltip_inline_action};
use crate::SettingsPanel;
use lumino_core::locale::LanguagePreference;
use lumino_core::storage::config::NoteCountDisplay;
use lumino_core::tr;
use lumino_extras::i18n::{Language, settings_translations};

/// 音符总量显示位置（本地化下拉项；UI-015）
#[derive(Clone)]
struct LocalizedNoteCountDisplay {
    inner: NoteCountDisplay,
    name: &'static str,
}

impl PartialEq for LocalizedNoteCountDisplay {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedNoteCountDisplay {}

impl std::fmt::Display for LocalizedNoteCountDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedNoteCountDisplay {
    fn new(value: NoteCountDisplay, lang: Language) -> Self {
        let t = settings_translations(lang);
        let name = match value {
            NoteCountDisplay::ProjectSettings => t.note_count_display_project_settings,
            NoteCountDisplay::StatusBar => t.note_count_display_statusbar,
        };
        Self { inner: value, name }
    }
}

/// 界面语言下拉项（UI-018）：在「具体语言」之外多一个「跟随系统」。
///
/// `Language` 本身只有两种具体语言，**不**为多一个选项去污染该枚举——那会打断全仓
/// 21 处 exhaustive `match`，并把「偏好」与「生效语言」混成一个类型。这里沿用同文件
/// [`LocalizedNoteCountDisplay`] 的包装范式：`PartialEq` 只比偏好、不比显示名，
/// 这样切换语言导致显示名变化时下拉选中态不会丢。
#[derive(Clone)]
struct LanguageOption {
    preference: LanguagePreference,
    name: String,
}

impl PartialEq for LanguageOption {
    fn eq(&self, other: &Self) -> bool {
        self.preference == other.preference
    }
}

impl Eq for LanguageOption {}

impl std::fmt::Display for LanguageOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LanguageOption {
    fn from_preference(preference: LanguagePreference, follow_system: &'static str) -> Self {
        let name = match preference {
            // 具体语言用**自称**（走 `Language` 的 Display，与既有下拉一致）
            LanguagePreference::Fixed(lang) => lang.to_string(),
            LanguagePreference::System => follow_system.to_string(),
        };
        Self { preference, name }
    }

    /// 完整选项列表：「跟随系统」在前，随后是全部具体语言。
    fn all(follow_system: &'static str) -> Vec<Self> {
        let mut options = vec![Self::from_preference(
            LanguagePreference::System,
            follow_system,
        )];
        options.extend(
            Language::all()
                .into_iter()
                .map(|lang| Self::from_preference(LanguagePreference::Fixed(lang), follow_system)),
        );
        options
    }
}

/// 渲染界面设置页面
pub fn view<'a>(
    settings: &'a SettingsPanel,
    window: &lumino_ui_core::window::Window,
    system_fonts: &[lumino_note_core::font_scanner::FontInfo],
) -> Element<'a> {
    let t = settings_translations(settings.display.language);

    // 创建复选框（说明挂在标签文字上，对钩框本身不触发）
    let native_titlebar_checkbox = if cfg!(target_os = "macos") {
        row![] // macOS 不需要这个选项
    } else {
        row![
            iced_widget::Checkbox::new(settings.synth.use_native_titlebar).on_toggle(|enabled| {
                Message::Settings(crate::Event::NativeTitlebarChanged(enabled))
            }),
            with_setting_tooltip_inline_action(
                text(t.native_titlebar)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.native_titlebar_hint,
                Message::Settings(crate::Event::NativeTitlebarChanged(
                    !settings.synth.use_native_titlebar,
                )),
            ),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center)
    };

    // 字体设置部分
    let font_section = font::build_font_section(settings, t, system_fonts);

    // 语言选项：「跟随系统」+ 全部具体语言（UI-018）
    let language_options = LanguageOption::all(t.language_follow_system);
    let current_language = LanguageOption::from_preference(
        settings.display.language_preference,
        t.language_follow_system,
    );

    column![
        text(t.ui_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 语言选择
        row![
            text(tr!("语言 / Language:"))
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(language_options, Some(current_language), |option| {
                Message::Settings(crate::Event::LanguagePreferenceChanged(option.preference))
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
        // HiDPI 图标渲染选项
        row![
            iced_widget::Checkbox::new(settings.display.icon_hidpi).on_toggle(|enabled| {
                Message::Settings(crate::Event::IconHiDPIChanged(enabled))
            }),
            with_setting_tooltip_inline_action(
                text(t.hidpi_icon)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.hidpi_icon_hint,
                Message::Settings(crate::Event::IconHiDPIChanged(!settings.display.icon_hidpi)),
            ),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(24),
        // 音符总量显示位置（UI-015）
        row![
            with_setting_tooltip_inline(
                text(t.note_count_display)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.note_count_display_hint,
            ),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(
                vec![
                    LocalizedNoteCountDisplay::new(
                        NoteCountDisplay::ProjectSettings,
                        settings.display.language
                    ),
                    LocalizedNoteCountDisplay::new(
                        NoteCountDisplay::StatusBar,
                        settings.display.language
                    ),
                ],
                Some(LocalizedNoteCountDisplay::new(
                    settings.display.note_count_display,
                    settings.display.language
                )),
                |value| Message::Settings(crate::Event::NoteCountDisplayChanged(value.inner)),
            )
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        // 自动滚动配置
        auto_scroll::build_auto_scroll_section(settings, t),
        // 框选框模式设置
        interaction::build_interaction_section(settings, t),
        iced_widget::space().height(24),
        // ── 自动化曲线连线粗细 section ──
        row![
            with_setting_tooltip_inline(
                text(t.ui_automation_line_thickness)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style())
                    .width(200.0),
                t.ui_automation_line_thickness_hint,
            ),
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
        iced_widget::space().height(24),
        // ── 底边栏监控数据刷新间隔 ──
        row![
            with_setting_tooltip_inline(
                text(t.ui_monitor_refresh_interval)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style())
                    .width(200.0),
                t.ui_monitor_refresh_interval_hint,
            ),
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
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
