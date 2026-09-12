//! 设置页 - 主题卡片预览
//!
//! 每张卡片按参照实现（yinhe 主题设置）的方式绘制迷你界面预览：
//! 卡片底色即该主题的背景色，左上角为主题名（主题主文字色），
//! 下方用主题的控件底色绘制「迷你面板」，内含主/次文字条、强调色条
//! 与两级分隔线，直观呈现该主题的配色层级。
//!
//! 卡片固定 158×96 比例（`Grid` 按可用宽度自适应列数），点击卡片即通过
//! `window::Event::Theme` 切换主题；选中态使用当前主题强调色 2px 描边。
//!
//! 卡片列表从主题注册表自动生成（高对比度 + `Theme::ALL`），
//! 后续新增主题无需改动本文件即可自动出现在列表中。

use iced_core::theme::palette::mix;
use iced_core::{Background, Border, Color, Length};
use iced_widget::grid::aspect_ratio;
use iced_widget::{Grid, button, column, container, space, text};
use lumino_extras::i18n::{Language, settings_translations};
use lumino_ui_core::theme::{HIGH_CONTRAST_DISPLAY, hc_theme};
use lumino_ui_core::{Element, Message, Theme, window};

/// 卡片宽度基准（像素）：Grid 按此宽度计算每行列数
const CARD_WIDTH: f32 = 158.0;
/// 卡片高度基准（像素）：与宽度共同决定卡片固定纵横比
const CARD_HEIGHT: f32 = 96.0;
/// 卡片间距（像素）
const CARD_SPACING: f32 = 12.0;
/// 卡片圆角半径
const CARD_RADIUS: f32 = 8.0;
/// 迷你面板圆角半径
const MOCK_RADIUS: f32 = 4.0;
/// 迷你面板内边距
const MOCK_PADDING: f32 = 6.0;
/// 迷你面板内文字条高度
const MOCK_BAR_HEIGHT: f32 = 4.0;
/// 迷你面板内分隔线高度
const MOCK_LINE_HEIGHT: f32 = 1.0;

/// 单张主题卡片的展示数据
struct ThemeCard {
    /// 卡片标注的主题名（高对比度使用本地化名称）
    display: String,
    /// 发送给 `window::Event::Theme` 的规范主题名
    value: String,
    /// 该主题实例，用于派生其预览配色
    theme: Theme,
}

/// 单张卡片的预览配色（全部由该主题的背景 / 文字 / 强调色派生）
#[derive(Clone, Copy)]
struct PreviewColors {
    /// 卡片底色（主题背景色）
    bg: Color,
    /// 卡片标题（主题主文字色）
    title: Color,
    /// 迷你面板底色（控件表面色）
    control: Color,
    /// 主文字条
    bar_primary: Color,
    /// 次级文字条
    bar_secondary: Color,
    /// 强调色条
    accent: Color,
    /// 分隔线
    line: Color,
    /// 次级分隔线（低透明度）
    line_soft: Color,
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

/// 渲染主题卡片网格（固定卡片纵横比、宽度自适应、自动换行）
///
/// `current` 为当前主题的规范名，与卡片 value 相等时该卡片渲染为选中态。
pub(crate) fn view(language: Language, current: &str) -> Element<'static> {
    let cards = theme_cards(language);
    Grid::with_children(
        cards
            .iter()
            .map(|card| card_button(card, card.value == current)),
    )
    // 固定 158:96 比例：Grid 的 height 是「网格总高」，直接给 Fixed 会按行均分
    // 造成卡片被压扁（历史 bug），必须用 aspect_ratio 保持单卡比例。
    .height(aspect_ratio(CARD_WIDTH, CARD_HEIGHT))
    .fluid(CARD_WIDTH)
    .spacing(CARD_SPACING)
    .into()
}

/// 构建单张主题卡片按钮
fn card_button(card: &ThemeCard, selected: bool) -> Element<'static> {
    let PreviewColors {
        bg,
        title,
        control,
        bar_primary,
        bar_secondary,
        accent,
        line,
        line_soft,
    } = preview_colors(&card.theme);

    // 迷你界面：控件底 + 主/次文字条 + 强调色条 + 两级分隔线
    let mock = container(
        column![
            mock_bar(bar_primary, 36.0),
            mock_bar(bar_secondary, 62.0),
            mock_bar(accent, 20.0),
            space().height(2),
            mock_line(line),
            mock_line(line_soft),
        ]
        .spacing(4)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(MOCK_PADDING)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(control)),
        border: Border::default().rounded(MOCK_RADIUS),
        ..container::Style::default()
    });

    let content = column![
        text(card.display.clone())
            .size(11.0)
            .width(Length::Fill)
            .style(move |_theme: &Theme| text::Style { color: Some(title) }),
        mock,
    ]
    .spacing(6);

    button(content)
        .on_press(Message::Window(window::Event::Theme(card.value.clone())))
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |theme: &Theme, status| card_style(theme, status, selected, bg, title))
        .into()
}

/// 迷你面板内的文字条（固定宽度、左对齐）
fn mock_bar(color: Color, width: f32) -> Element<'static> {
    container(space())
        .width(Length::Fixed(width))
        .height(Length::Fixed(MOCK_BAR_HEIGHT))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(color)),
            border: Border::default().rounded(2.0),
            ..container::Style::default()
        })
        .into()
}

/// 迷你面板内的分隔线（占满宽度）
fn mock_line(color: Color) -> Element<'static> {
    container(space())
        .width(Length::Fill)
        .height(Length::Fixed(MOCK_LINE_HEIGHT))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(color)),
            ..container::Style::default()
        })
        .into()
}

/// 卡片按钮样式：底色恒为主题自身背景色；选中态为当前主题强调色 2px 描边，
/// 悬浮态加亮描边（参照实现的三段描边规则）。
fn card_style(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    card_bg: Color,
    card_text: Color,
) -> button::Style {
    let current = preview_colors(theme);
    let (width, color) = if selected {
        (2.0, current.accent)
    } else {
        match status {
            button::Status::Hovered | button::Status::Pressed => {
                (1.5, with_alpha(current.line, 0.9))
            }
            _ => (1.0, with_alpha(current.line, 0.45)),
        }
    };
    button::Style {
        background: Some(Background::Color(card_bg)),
        text_color: card_text,
        border: Border::default()
            .rounded(CARD_RADIUS)
            .width(width)
            .color(color),
        shadow: Default::default(),
        snap: false,
    }
}

// ── 预览色派生（与参照实现同规则）────────────────────────────

/// 感知亮度（Rec.601），用于判定主题明暗
fn luminance(color: Color) -> f32 {
    0.299 * color.r + 0.587 * color.g + 0.114 * color.b
}

/// 表面色：暗色底向文字色提亮、亮色底向黑色压暗（两套幅度各自校准）
fn derive_surface(bg: Color, text: Color, dark_amount: f32, light_amount: f32) -> Color {
    if luminance(bg) <= 0.5 {
        mix(bg, text, dark_amount)
    } else {
        mix(bg, Color::BLACK, light_amount)
    }
}

/// 次级文字：暗色底向黑衰减、亮色底向背景靠拢（避免亮色主题灰阶坍缩）
fn derive_secondary_text(bg: Color, text: Color) -> Color {
    if luminance(bg) <= 0.5 {
        mix(text, Color::BLACK, 0.18)
    } else {
        mix(text, bg, 0.14)
    }
}

/// 从主题派生完整预览配色
fn preview_colors(theme: &Theme) -> PreviewColors {
    let palette = theme.extended_palette();
    let bg = palette.background.base.color;
    let title = palette.background.base.text;
    let accent = palette.primary.base.color;
    // 控件表面 / 分隔线幅度：暗色主题向文字提亮 5% / 18%，亮色主题向黑压暗 3% / 7%
    let control = derive_surface(bg, title, 0.05, 0.03);
    let line = derive_surface(bg, title, 0.18, 0.07);
    PreviewColors {
        bg,
        title,
        control,
        bar_primary: title,
        bar_secondary: derive_secondary_text(bg, title),
        accent,
        line,
        line_soft: with_alpha(line, 110.0 / 255.0),
    }
}

/// 替换颜色透明度（保留 RGB）
fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
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

    /// 暗色主题派生：控件面与线条应逐级提亮于背景，且卡片底色即主题背景色
    #[test]
    fn test_preview_colors_dark_theme_layers_lighten() {
        let c = preview_colors(&hc_theme());
        assert_eq!(c.bg, Color::BLACK, "卡片底色应为主题背景色");
        assert_eq!(c.title, Color::WHITE, "标题应为主题主文字色");
        assert!(
            luminance(c.control) > luminance(c.bg),
            "暗色主题控件面应比背景更亮"
        );
        assert!(
            luminance(c.line) > luminance(c.control),
            "暗色主题分隔线应比控件面更亮"
        );
        assert!(
            luminance(c.bar_secondary) > luminance(c.bg),
            "暗色主题次级文字应亮于背景"
        );
    }

    /// 亮色主题派生：控件面与线条应逐级压暗于背景，次级文字仍深于背景
    #[test]
    fn test_preview_colors_light_theme_layers_darken() {
        let c = preview_colors(&Theme::Light);
        assert!(
            luminance(c.control) < luminance(c.bg),
            "亮色主题控件面应比背景更暗"
        );
        assert!(
            luminance(c.line) < luminance(c.control),
            "亮色主题分隔线应比控件面更暗"
        );
        assert!(
            luminance(c.bar_secondary) < luminance(c.bg),
            "亮色主题次级文字应深于背景"
        );
    }

    /// 强调色条必须取自主题自身 primary 色
    #[test]
    fn test_preview_accent_matches_theme_primary() {
        for theme in [Theme::Dracula, Theme::Light, Theme::TokyoNightStorm] {
            let c = preview_colors(&theme);
            assert_eq!(
                c.accent,
                theme.extended_palette().primary.base.color,
                "强调色条应取自主题 primary"
            );
            assert_eq!(
                c.bg,
                theme.extended_palette().background.base.color,
                "卡片底色应取自主题 background"
            );
        }
    }
}
