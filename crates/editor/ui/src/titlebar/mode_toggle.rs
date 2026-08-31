use iced_core::{Alignment, Background, Border, Color, Length};
use iced_widget::{button, container, row, text};

use crate::widget;
use crate::{Element, Message, Theme, resources::icon};
use lumino_extras::i18n::{Language, main_translations};

pub use lumino_ui_core::app_mode::AppMode;

/// 渲染标题栏模式切换视图
pub fn view(
    theme: &Theme,
    current_mode: AppMode,
    progress: f32,
    language: Language,
) -> Element<'_> {
    let t = main_translations(language);
    let p = progress.clamp(0.0, 1.0);
    let palette = theme.extended_palette();

    // 三态：Editor(0) / Yinhe(0.5) / Waterfall(1)，progress 为动画插值，current_mode 为目标态
    let (icon_type, label, tooltip_text) = if current_mode == AppMode::Yinhe {
        // yinhe 需 --features yinhe，无 feature 时不会进入此分支（但仍编译期可见）
        (icon::Icon::Arrangement, "Yinhe", "切换到瀑布流 (Yinhe→Waterfall)")
    } else if current_mode == AppMode::Waterfall || p >= 0.75 {
        (icon::Icon::Keys, t.mode_waterfall, t.mode_switch_to_editor)
    } else if p >= 0.25 && current_mode != AppMode::Editor {
        // 动画中途按目标态显示
        (icon::Icon::Arrangement, "Yinhe", "切换到瀑布流")
    } else {
        (icon::Icon::PencilOutline, t.mode_editor, t.mode_switch_to_waterfall)
    };
    let is_waterfall = current_mode == AppMode::Waterfall;

    let icon_bg = palette.background.strong.color;
    let text_color = palette.background.neutral.text;

    let icon_el = container(icon::view_with_size_and_theme(
        icon_type,
        13,
        13,
        Some(theme),
    ))
    .width(Length::Fixed(17.0))
    .height(Length::Fixed(17.0))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(icon_bg)),
        ..Default::default()
    });

    let text_alpha = if is_waterfall { p } else { 1.0 - p };

    let label_text = text(label)
        .size(12)
        .style(move |_theme: &Theme| text::Style {
            color: Some(Color::from_rgba8(
                (text_color.r * 255.0) as u8,
                (text_color.g * 255.0) as u8,
                (text_color.b * 255.0) as u8,
                text_alpha,
            )),
        });

    let content = if is_waterfall {
        row![label_text, icon_el]
    } else {
        row![icon_el, label_text]
    }
    .spacing(4)
    .align_y(Alignment::Center);

    let inner_bg = palette.background.weaker.color;
    let outer_bg = palette.background.weak.color;
    let border_radius = 4.0;

    let inner = container(content)
        .padding([2, 5])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(inner_bg)),
            ..Default::default()
        });

    let btn = button(inner)
        .padding(2)
        .style(move |_theme: &Theme, status| {
            let bg = match status {
                button::Status::Hovered => outer_bg,
                button::Status::Pressed => palette.background.base.color,
                _ => outer_bg,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: border_radius.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                text_color,
                ..Default::default()
            }
        })
        .on_press(Message::ModeToggled)
        .width(70)
        .height(25);

    widget::with_tooltip_bottom(btn, tooltip_text).into()
}
