//! 设置页 - 主题卡片预览
//!
//! 每个主题一张卡片，卡片内直接绘制该主题的配色色块，卡片标注主题名。
//! 点击卡片即通过 `window::Event::Theme` 切换主题，选中卡片以高亮边框
//! 与主色标注区分。
//!
//! 卡片列表从主题注册表自动生成（高对比度 + `Theme::ALL`），
//! 后续新增主题无需改动本文件即可自动出现在列表中。

use iced_core::{Background, Border, Color, Length};
use iced_widget::{Grid, button, column, container, row, space, text};
use lumino_extras::i18n::{Language, settings_translations};
use lumino_ui_core::theme::{HIGH_CONTRAST_DISPLAY, hc_theme};
use lumino_ui_core::{Element, Message, Theme, window};

/// 卡片宽度上限（像素）：Grid 按此宽度计算每行列数，实现宽度自适应
const CARD_MAX_WIDTH: f32 = 168.0;
/// 卡片整体高度（像素）
const CARD_HEIGHT: f32 = 104.0;
/// 卡片内色块区域高度（像素）
const SWATCH_HEIGHT: f32 = 62.0;
/// 卡片间距（像素）
const CARD_SPACING: f32 = 10.0;
/// 卡片圆角半径
const CARD_RADIUS: f32 = 8.0;
/// 色块圆角半径
const SWATCH_RADIUS: f32 = 6.0;

/// 单张主题卡片的展示数据
struct ThemeCard {
    /// 卡片标注的主题名（高对比度使用本地化名称）
    display: String,
    /// 发送给 `window::Event::Theme` 的规范主题名
    value: String,
    /// 该主题实例，用于取其配色色块
    theme: Theme,
}

/// 从主题注册表生成卡片列表。
///
/// 高对比度主题在注册表之外（自定义 Custom 主题），单独置于首位；
/// 其余卡片直接遍历 `Theme::ALL` 生成，新增主题零改动自动出现。
fn theme_cards(language: Language) -> Vec<ThemeCard> {
    let t = settings_translations(language);
    let mut cards = Vec::with_capacity(Theme::ALL.len() + 1);
    cards.push(ThemeCard {
        display: t.high_contrast.to_string(),
        value: HIGH_CONTRAST_DISPLAY.to_string(),
        theme: hc_theme(),
    });
    cards.extend(Theme::ALL.iter().map(|theme| ThemeCard {
        display: theme.to_string(),
        value: theme.to_string(),
        theme: theme.clone(),
    }));
    cards
}

/// 渲染主题卡片网格（宽度自适应、自动换行）
///
/// `current` 为当前主题的规范名，与卡片 value 相等时该卡片渲染为选中态。
pub(crate) fn view(language: Language, current: &str) -> Element<'static> {
    let cards = theme_cards(language);
    Grid::with_children(
        cards
            .iter()
            .map(|card| card_button(card, card.value == current)),
    )
    .fluid(CARD_MAX_WIDTH)
    .spacing(CARD_SPACING)
    .height(Length::Fixed(CARD_HEIGHT))
    .into()
}

/// 构建单张主题卡片按钮
fn card_button(card: &ThemeCard, selected: bool) -> Element<'static> {
    // `extended_palette` 返回引用，Color 为 Copy —— 先取出具体颜色，
    // 使样式闭包不携带对 card 的借用（返回 'static Element 的前提）。
    let palette = card.theme.extended_palette();
    let background = palette.background.base.color;
    let primary = palette.primary.base.color;
    let success = palette.success.base.color;
    let danger = palette.danger.base.color;

    // 该主题的配色色块：背景 / 强调 / 成功 / 警示
    let swatches = row![
        color_block(background),
        color_block(primary),
        color_block(success),
        color_block(danger),
    ]
    .spacing(1)
    .width(Length::Fill)
    .height(Length::Fill);

    button(
        column![
            container(swatches)
                .width(Length::Fill)
                .height(Length::Fixed(SWATCH_HEIGHT))
                .clip(true)
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Background::Color(background)),
                    border: Border::default().rounded(SWATCH_RADIUS),
                    ..container::Style::default()
                }),
            space().height(6),
            text(card.display.clone())
                .size(12.0)
                .center()
                .width(Length::Fill)
                .style(move |theme: &Theme| {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(if selected {
                            palette.primary.base.color
                        } else {
                            palette.background.base.text
                        }),
                    }
                }),
        ]
        .spacing(0),
    )
    .on_press(Message::Window(window::Event::Theme(card.value.clone())))
    .padding(8)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |theme: &Theme, status| card_style(theme, status, selected))
    .into()
}

/// 单个色块（等分宽度、填充高度）
fn color_block(color: Color) -> Element<'static> {
    container(space())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(color)),
            ..container::Style::default()
        })
        .into()
}

/// 卡片按钮样式：选中态使用当前主题主色 2px 高亮边框
fn card_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let palette = theme.extended_palette();
    let border = if selected {
        Border::default()
            .rounded(CARD_RADIUS)
            .width(2.0)
            .color(palette.primary.base.color)
    } else {
        Border::default()
            .rounded(CARD_RADIUS)
            .width(1.0)
            .color(palette.background.strong.color)
    };
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => {
            Some(Background::Color(palette.background.weak.color))
        }
        _ => Some(Background::Color(palette.background.base.color)),
    };
    button::Style {
        background,
        text_color: palette.background.base.text,
        border,
        shadow: Default::default(),
        snap: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 卡片列表必须由注册表自动生成：高对比度 + 全部内置主题，无遗漏
    #[test]
    fn test_theme_cards_generated_from_registry() {
        let cards = theme_cards(Language::ZhCn);
        assert_eq!(
            cards.len(),
            Theme::ALL.len() + 1,
            "卡片数应为 高对比度 + Theme::ALL 全部主题"
        );
        assert_eq!(
            cards[0].value, HIGH_CONTRAST_DISPLAY,
            "首张卡片应为高对比度"
        );
        let expected: Vec<String> = Theme::ALL.iter().map(|t| t.to_string()).collect();
        let actual: Vec<String> = cards[1..].iter().map(|c| c.value.clone()).collect();
        assert_eq!(actual, expected, "内置主题卡片应与 Theme::ALL 一一对应");
    }

    /// 卡片 value 不应重复（避免选中态匹配到多张卡片）
    #[test]
    fn test_theme_cards_values_unique() {
        let cards = theme_cards(Language::ZhCn);
        let mut values: Vec<&str> = cards.iter().map(|c| c.value.as_str()).collect();
        values.sort_unstable();
        let count = values.len();
        values.dedup();
        assert_eq!(values.len(), count, "主题卡片的 value 不应重复");
    }

    /// 高对比度卡片使用本地化显示名，但 value 保持注册表规范名不变
    #[test]
    fn test_high_contrast_card_localized_display_keeps_canonical_value() {
        let zh = theme_cards(Language::ZhCn);
        let en = theme_cards(Language::EnUs);
        assert_eq!(zh[0].value, HIGH_CONTRAST_DISPLAY);
        assert_eq!(en[0].value, HIGH_CONTRAST_DISPLAY);
        assert_eq!(zh[0].display, "高对比度");
        assert_eq!(en[0].display, "High Contrast");
    }
}
