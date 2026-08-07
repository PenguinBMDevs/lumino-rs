//! 设置页面 - 编辑设置
//!
//! 包含操作历史（撤销/重做日志上限、合并窗口）、编辑拦截（Toast 提示）
//! 以及 Tempo 面板 BPM 绘制上限（预设下拉 + 自定义输入弹窗）。

use iced_core::{Alignment, Color, Length};
use iced_widget::{
    Space, Stack, button, column, container, mouse_area, pick_list, row, space, text, text_input,
};
use lumino_extras::i18n::settings_translations;
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::SettingsPanel;

/// Tempo BPM 上限预设值
const TEMPO_MAX_BPM_PRESETS: [f64; 9] = [
    256.0, 512.0, 1024.0, 2048.0, 4096.0, 8192.0, 16384.0, 32767.0, 65536.0,
];

/// Tempo BPM 上限下拉选项：预设值 + 自定义
#[derive(Debug, Clone, Copy, PartialEq)]
enum TempoMaxBpmOption {
    Preset(f64),
    Custom,
}

impl TempoMaxBpmOption {
    /// 当前 BPM 上限对应的下拉选项
    ///
    /// 若当前值恰好等于某个预设值则显示该预设，否则显示"自定义"。
    fn from_bpm(bpm: f64) -> Self {
        TEMPO_MAX_BPM_PRESETS
            .iter()
            .copied()
            .find(|&p| (p - bpm).abs() < f64::EPSILON)
            .map(Self::Preset)
            .unwrap_or(Self::Custom)
    }

    fn all() -> Vec<Self> {
        let mut options: Vec<Self> = TEMPO_MAX_BPM_PRESETS
            .iter()
            .copied()
            .map(Self::Preset)
            .collect();
        options.push(Self::Custom);
        options
    }
}

/// 本地化 Tempo BPM 上限下拉选项（支持按语言显示"自定义"）
#[derive(Debug, Clone, Copy)]
struct LocalizedTempoOption {
    inner: TempoMaxBpmOption,
    custom_label: &'static str,
}

impl PartialEq for LocalizedTempoOption {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl std::fmt::Display for LocalizedTempoOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.inner {
            TempoMaxBpmOption::Preset(v) => write!(f, "{v:.0}"),
            TempoMaxBpmOption::Custom => write!(f, "{}", self.custom_label),
        }
    }
}

impl LocalizedTempoOption {
    fn new(option: TempoMaxBpmOption, custom_label: &'static str) -> Self {
        Self {
            inner: option,
            custom_label,
        }
    }
}

/// 渲染编辑设置页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.language);

    let content = column![
        text(t.editing_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // ── 操作历史 section ──
        text(t.editing_history_section)
            .size(TEXT_SIZE_SECTION)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 操作日志总条数上限
        row![
            text(t.editing_history_total_limit)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("100", &settings.history_total_limit.to_string())
                .on_input(|v| Message::Settings(crate::Event::HistoryTotalLimitChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_history_total_limit_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 单条日志条目上限
        row![
            text(t.editing_history_entry_limit)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("1000", &settings.history_entry_limit.to_string())
                .on_input(|v| Message::Settings(crate::Event::HistoryEntryLimitChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_history_entry_limit_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 合并窗口
        row![
            text(t.editing_merge_window)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input("300", &settings.merge_window_ms.to_string())
                .on_input(|v| Message::Settings(crate::Event::MergeWindowMsChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_merge_window_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // ── Tempo 面板 section ──
        text(t.editing_tempo_max_bpm)
            .size(TEXT_SIZE_SECTION)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // BPM 绘制上限下拉（预设 + 自定义）
        row![
            text(t.editing_tempo_max_bpm)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(
                TempoMaxBpmOption::all()
                    .into_iter()
                    .map(|opt| LocalizedTempoOption::new(opt, t.editing_tempo_custom_option))
                    .collect::<Vec<_>>(),
                Some(LocalizedTempoOption::new(
                    TempoMaxBpmOption::from_bpm(settings.tempo_max_bpm),
                    t.editing_tempo_custom_option,
                )),
                |option| match option.inner {
                    TempoMaxBpmOption::Preset(v) => {
                        Message::Settings(crate::Event::TempoMaxBpmChanged(v))
                    }
                    TempoMaxBpmOption::Custom => {
                        Message::Settings(crate::Event::TempoMaxBpmCustomOpen)
                    }
                },
            )
            .width(Length::Fixed(120.0)),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_tempo_max_bpm_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // ── 编辑拦截 section ──
        text(t.editing_intercept_section)
            .size(TEXT_SIZE_SECTION)
            .style(create_content_text_style()),
        iced_widget::space().height(SPACING_CONTENT),
        // 拦截时显示 Toast 提示
        row![
            iced_widget::Checkbox::new(settings.intercept_notification_enabled)
                .label(t.editing_intercept_notification)
                .on_toggle(|enabled| {
                    Message::Settings(crate::Event::InterceptNotificationChanged(enabled))
                }),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(4),
        text(t.editing_intercept_notification_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT);

    if settings.tempo_custom_open {
        // 自定义 BPM 上限弹窗：遮罩 + 居中输入卡片
        Stack::new()
            .push(custom_bpm_overlay(settings))
            .push(
                container(custom_bpm_card(settings))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced_core::alignment::Horizontal::Center)
                    .align_y(iced_core::alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        content.into()
    }
}

/// 自定义 BPM 上限弹窗的遮罩层：点击外部区域关闭
fn custom_bpm_overlay<'a>(_settings: &SettingsPanel) -> Element<'a> {
    container(
        mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
            .on_press(Message::Settings(crate::Event::TempoMaxBpmCustomClose)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme: &lumino_ui_core::Theme| container::Style {
        background: Some(iced_core::Background::Color(Color::from_rgba(
            0.0, 0.0, 0.0, 0.45,
        ))),
        ..Default::default()
    })
    .into()
}

/// 自定义 BPM 上限弹窗的输入卡片
fn custom_bpm_card<'a>(settings: &SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.language);

    let confirm_btn = button(text(t.confirm).size(13))
        .on_press(Message::Settings(crate::Event::TempoMaxBpmCustomConfirm))
        .padding([6, 20]);
    let cancel_btn = button(text(t.cancel).size(13))
        .on_press(Message::Settings(crate::Event::TempoMaxBpmCustomClose))
        .padding([6, 20]);

    container(
        column![
            text(t.editing_tempo_custom_title)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().height(12),
            text_input(
                t.editing_tempo_custom_placeholder,
                &settings.tempo_custom_input
            )
            .on_input(|v| Message::Settings(crate::Event::TempoMaxBpmCustomInput(v)))
            .padding([6, 10])
            .width(Length::Fixed(220.0)),
            iced_widget::space().height(16),
            row![confirm_btn, space().width(8), cancel_btn]
                .spacing(SPACING_ICON_LABEL)
                .align_y(Alignment::Center),
        ]
        .align_x(Alignment::Start),
    )
    .padding(20)
    .style(|theme: &lumino_ui_core::Theme| container::Style {
        background: Some(iced_core::Background::Color(
            theme.extended_palette().background.base.color,
        )),
        border: iced_core::Border::default()
            .rounded(8)
            .width(1)
            .color(theme.extended_palette().background.strong.color),
        ..Default::default()
    })
    .into()
}
