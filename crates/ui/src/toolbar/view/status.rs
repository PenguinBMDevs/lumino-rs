//! 工具栏状态显示函数
//!
//! 包含精度选择、自动滚动按钮、协作按钮等状态相关渲染。

use iced_core::Alignment;
use iced_widget::{button, container, pick_list, row, space, text};

use crate::message::CustomPrecisionAction;
use crate::resources::icon;
use crate::toolbar::{ButtonId, Event, NotePrecision, Toolbar};
use crate::widget;
use crate::{Element, Message, Theme, window};
use lumino_extras::i18n::{Language, MainTranslations};

/// 本地化音符精度包装（支持按语言显示名称）
#[derive(Debug, Clone, Copy)]
struct LocalizedPrecision {
    inner: NotePrecision,
    name: &'static str,
}

impl PartialEq for LocalizedPrecision {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedPrecision {}

impl std::fmt::Display for LocalizedPrecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedPrecision {
    fn new(precision: NotePrecision, lang: Language) -> Self {
        Self {
            inner: precision,
            name: lumino_extras::i18n::note_precision_name(precision, lang),
        }
    }
}

impl Toolbar {
    /// 渲染精度选择器内容（label + pick_list），不含外层容器。
    ///
    /// 设计为可嵌入其他工具栏框（如工具选择区），因此不自带背景/内边距，
    /// 由外层框统一控制外观，实现"精度下拉与笔工具在同一个框内"。
    pub fn render_precision_selector<'a>(
        &'a self,
        _content_height: f32,
        _palette: &'a iced_core::theme::palette::Extended,
        language: Language,
        t: &'static MainTranslations,
    ) -> Element<'a> {
        let precision_options: Vec<LocalizedPrecision> = NotePrecision::presets()
            .iter()
            .copied()
            .chain(std::iter::once(NotePrecision::Custom))
            .map(|p| LocalizedPrecision::new(p, language))
            .collect();
        let current_precision = LocalizedPrecision::new(self.note_precision, language);

        row![
            text(t.precision_label).size(14),
            space().width(8),
            pick_list(precision_options, Some(current_precision), |lp| {
                if lp.inner == NotePrecision::Custom {
                    // 选择自定义时，发送消息到Root打开对话框
                    Message::CustomPrecision(CustomPrecisionAction::OpenDialog)
                } else {
                    Event::precision_changed(lp.inner)
                }
            },)
            .placeholder(t.precision_placeholder)
            .padding([4, 8])
            .width(iced_widget::core::Length::Fixed(120.0)),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    /// 渲染自动滚动模式切换按钮（图标 + tooltip，无常驻文字）
    pub fn render_auto_scroll_button<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        use lumino_core::storage::config::AutoScrollMode;
        let auto_scroll_icon = match self.auto_scroll_mode {
            AutoScrollMode::FixedIndicatorLeft => icon::ArrowsLeftRight,
            AutoScrollMode::ScrollingIndicator => icon::Scroll,
            AutoScrollMode::Off => icon::Ban,
        };
        container(widget::with_tooltip_bottom(
            iced_widget::mouse_area(
                button(icon::view_with_size_and_theme(
                    auto_scroll_icon,
                    18,
                    18,
                    Some(&window.theme),
                ))
                .on_press(Event::auto_scroll_mode_changed())
                .style(move |_theme: &Theme, status| {
                    let bg = match status {
                        iced_widget::button::Status::Hovered => palette.background.weak.color,
                        _ => palette.background.weakest.color,
                    };
                    button::Style {
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        ..Default::default()
                    }
                    .with_background(bg)
                })
                .padding([8, 8]),
            )
            .on_enter(Event::button_hovered(Some(ButtonId::AutoScroll)))
            .on_exit(Event::button_hovered(None)),
            t.auto_scroll_tooltip,
        ))
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding([0, 4])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        })
        .into()
    }

    /// 渲染协作按钮（图标 + tooltip，无常驻文字）
    pub fn render_collaboration_button<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        container(widget::with_tooltip_bottom(
            iced_widget::mouse_area(
                button(icon::view_with_size_and_theme(
                    icon::Users,
                    18,
                    18,
                    Some(&window.theme),
                ))
                .on_press(Event::open_collaboration_dialog())
                .style(move |_theme: &Theme, status| {
                    let bg = match status {
                        iced_widget::button::Status::Hovered => palette.background.weak.color,
                        _ => palette.background.weakest.color,
                    };
                    button::Style {
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        ..Default::default()
                    }
                    .with_background(bg)
                })
                .padding([8, 8]),
            )
            .on_enter(Event::button_hovered(Some(ButtonId::Collaboration)))
            .on_exit(Event::button_hovered(None)),
            t.collaboration_tooltip,
        ))
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding([0, 4])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        })
        .into()
    }
}
