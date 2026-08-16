//! 设置页面 - 常规设置

use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text, text_input};
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
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
    let current_eraser = LocalizedEraser::new(settings.editing.eraser_behavior, settings.display.language);

    column![
        text(t.general_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 橡皮擦行为选择
        row![
            text(t.eraser_behavior)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(eraser_options, Some(current_eraser), |le| {
                Message::Settings(crate::Event::EraserBehaviorChanged(le.inner))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text(t.eraser_default_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        text(t.eraser_direct_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(20),
        // 添加音轨行为选择
        row![
            text(t.track_add_behavior)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(
                vec![
                    LocalizedTrackAdd::new(TrackAddBehavior::AutoSwitch, settings.display.language),
                    LocalizedTrackAdd::new(TrackAddBehavior::StayCurrent, settings.display.language),
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
        iced_widget::space().height(SPACING_CONTENT),
        text(t.track_add_behavior_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(20),
        // 日志存储份数设置
        text(t.log_retention_section)
            .size(TEXT_SIZE_SECTION)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        row![
            text(t.log_retention_count)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("10", &settings.logging.log_retention_count.to_string())
                .on_input(|v| Message::Settings(crate::Event::LogRetentionCountChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.log_retention_count_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
