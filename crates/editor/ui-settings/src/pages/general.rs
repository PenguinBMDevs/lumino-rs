//! 设置页面 - 常规设置

use iced_core::Alignment;
use iced_widget::{checkbox, column, pick_list, row, text, text_input};
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::create_content_text_style;
use super::super::components::{with_setting_tooltip_inline, with_setting_tooltip_inline_action};
use crate::SettingsPanel;
use lumino_core::storage::config::{EraserBehavior, TrackAddBehavior};
use lumino_extras::i18n::settings_translations;

/// 本地化橡皮擦行为包装
#[derive(Debug, Clone, Copy)]
struct LocalizedEraser {
    inner: EraserBehavior,
    name: &'static str,
}

impl PartialEq for LocalizedEraser {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedEraser {}

impl std::fmt::Display for LocalizedEraser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedEraser {
    fn new(behavior: EraserBehavior, lang: lumino_extras::i18n::Language) -> Self {
        Self {
            inner: behavior,
            name: lumino_extras::i18n::eraser_behavior_name(behavior, lang),
        }
    }
}

/// 本地化音轨添加行为包装
#[derive(Debug, Clone, Copy)]
struct LocalizedTrackAdd {
    inner: TrackAddBehavior,
    name: &'static str,
}

impl PartialEq for LocalizedTrackAdd {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedTrackAdd {}

impl std::fmt::Display for LocalizedTrackAdd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedTrackAdd {
    fn new(behavior: TrackAddBehavior, lang: lumino_extras::i18n::Language) -> Self {
        Self {
            inner: behavior,
            name: lumino_extras::i18n::track_add_behavior_name(behavior, lang),
        }
    }
}

/// 渲染常规设置页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.display.language);

    // 橡皮擦行为选项（本地化）
    let eraser_options = vec![
        LocalizedEraser::new(EraserBehavior::Default, settings.display.language),
        LocalizedEraser::new(EraserBehavior::DirectSelect, settings.display.language),
    ];
    let current_eraser =
        LocalizedEraser::new(settings.editing.eraser_behavior, settings.display.language);

    // 看门狗在 macOS 上整体禁用（系统内存压力模型不同，见 memory_monitor/watchdog.rs）：
    // 与 Domino 开关同款处理——对钩框不挂 on_toggle（置灰），标签改用无动作 tooltip，
    // 避免出现一个「能点但永远不会生效」的假开关。
    let watchdog_checkbox = checkbox(settings.logging.load_watchdog_enabled);
    #[cfg(not(target_os = "macos"))]
    let watchdog_checkbox = watchdog_checkbox
        .on_toggle(|enabled| Message::Settings(crate::Event::LoadWatchdogEnabledChanged(enabled)));
    let watchdog_label = text(t.load_watchdog)
        .size(TEXT_SIZE_CONTENT)
        .style(create_content_text_style());
    #[cfg(not(target_os = "macos"))]
    let watchdog_label = with_setting_tooltip_inline_action(
        watchdog_label,
        t.load_watchdog_hint,
        Message::Settings(crate::Event::LoadWatchdogEnabledChanged(
            !settings.logging.load_watchdog_enabled,
        )),
    );
    #[cfg(target_os = "macos")]
    let watchdog_label =
        with_setting_tooltip_inline(watchdog_label, t.load_watchdog_unsupported_hint);

    column![
        text(t.general_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 橡皮擦行为选择
        // 两条说明（默认 / 直接框选）分别挂在设置项内的标签与下拉控件上，
        // 避免嵌套 tooltip 导致两个提示在同一锚点重叠显示。
        row![
            with_setting_tooltip_inline(
                text(t.eraser_behavior)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.eraser_default_hint,
            ),
            iced_widget::space().width(SPACING_MAIN),
            with_setting_tooltip_inline(
                pick_list(eraser_options, Some(current_eraser), |le| {
                    Message::Settings(crate::Event::EraserBehaviorChanged(le.inner))
                })
                .width(200.0),
                t.eraser_direct_hint,
            ),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(20),
        // 添加音轨行为选择
        row![
            with_setting_tooltip_inline(
                text(t.track_add_behavior)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.track_add_behavior_hint,
            ),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(
                vec![
                    LocalizedTrackAdd::new(TrackAddBehavior::AutoSwitch, settings.display.language),
                    LocalizedTrackAdd::new(
                        TrackAddBehavior::StayCurrent,
                        settings.display.language
                    ),
                ],
                Some(LocalizedTrackAdd::new(
                    settings.editing.track_add_behavior,
                    settings.display.language
                )),
                |lt| Message::Settings(crate::Event::TrackAddBehaviorChanged(lt.inner)),
            )
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(20),
        // 日志存储份数设置
        text(t.log_retention_section)
            .size(TEXT_SIZE_SECTION)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        row![
            with_setting_tooltip_inline(
                text(t.log_retention_count)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.log_retention_count_hint,
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("10", &settings.logging.log_retention_count.to_string())
                .on_input(|v| Message::Settings(crate::Event::LogRetentionCountChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(20),
        // ── 加载保护（默认关闭）──
        // 加载大工程/黑乐谱时的内存检查与强杀看门狗默认都不启用：
        // 由用户显式选择「宁可慢/被系统 OOM」还是「由程序主动终止」。
        text(t.load_guard_section)
            .size(TEXT_SIZE_SECTION)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 加载期间内存检查（说明挂在标签文字上，点击文字等价于切换对钩）
        row![
            checkbox(settings.logging.load_memory_check_enabled).on_toggle(|enabled| {
                Message::Settings(crate::Event::LoadMemoryCheckEnabledChanged(enabled))
            }),
            with_setting_tooltip_inline_action(
                text(t.load_memory_check)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.load_memory_check_hint,
                Message::Settings(crate::Event::LoadMemoryCheckEnabledChanged(
                    !settings.logging.load_memory_check_enabled,
                )),
            ),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(8),
        // 加载期间进程强杀看门狗（说明挂在标签文字上，点击文字等价于切换对钩）
        row![watchdog_checkbox, watchdog_label]
            .spacing(SPACING_ICON_LABEL)
            .align_y(Alignment::Center),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
