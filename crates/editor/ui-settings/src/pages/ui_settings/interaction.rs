//! 界面设置 - 交互设置部分（框选框模式、256 键钢琴卷帘、力度面板样式、播放键盘颜色）

use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text};
use lumino_ui_core::{Element, Message};

use super::super::super::components::constants::*;
use super::super::super::components::styles::{
    create_content_text_style, create_placeholder_text_style,
};
use crate::SettingsPanel;
use lumino_core::storage::config::SelectionBoxMode;
use lumino_extras::i18n::{Language, SettingsTranslations};

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

/// 构建交互设置部分（框选框模式、256 键钢琴卷帘、力度面板样式、播放键盘颜色）
pub(crate) fn build_interaction_section<'a>(
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
