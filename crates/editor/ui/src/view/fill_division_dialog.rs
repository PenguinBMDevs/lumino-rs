//! 颜料桶「分音符填充」对话框（主窗口内嵌覆盖层）
//!
//! 触发：填充桶开启时，画布上 **Ctrl + 单击** → `Editor` 置请求位 →
//! `Root` 打开本覆盖层。
//!
//! 交互：
//! - 输入框：填写 x（**任意数字**，不限于 2 的幂）→ 以 x 分音符切分填充区域；
//! - [确定] → 写入 `LineToolState::fill_division`；
//! - [取消] / 点击遮罩 → 关闭，不改档位。
//!
//! 与主窗口素材删除确认框（`right_sidebar/material_delete_dialog.rs`）同范式：
//! 全屏半透明遮罩 + 居中卡片，不走 runner / 独立 OS 窗口。

use iced_core::{Alignment, Color, Length};
use iced_widget::{Space, button, column, container, mouse_area, row, text, text_input};
use lumino_extras::i18n::{Language, main_translations};

use crate::state::root_state::FillDivisionDialogState;
use crate::{Element, Message, Theme};
use crate::{message::FillDivisionAction as Action, message::Message as AppMessage};

/// 遮罩半透明黑色
const MASK_BACKGROUND: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.45);
/// 卡片宽度（X 向）
const CARD_WIDTH: f32 = 380.0;

/// 渲染「分音符填充」对话框（全屏遮罩 + 居中卡片）
pub fn view_fill_division_dialog(
    state: &FillDivisionDialogState,
    language: Language,
) -> Element<'static> {
    let t = main_translations(language);

    // 全屏半透明遮罩：点击关闭
    let mask: Element<'static> = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill)).style(|_theme: &Theme| {
            container::Style {
                background: Some(iced_core::Background::Color(MASK_BACKGROUND)),
                ..Default::default()
            }
        }),
    )
    .on_press(AppMessage::FillDivision(Action::CloseDialog))
    .into();

    // 居中卡片（mouse_area 吞掉卡片区域点击，避免触发遮罩关闭）
    let card: Element<'static> = mouse_area(dialog_card(&state.value, t))
        .on_press(Message::Null)
        .into();

    iced_widget::Stack::new()
        .push(mask)
        .push(
            container(card)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// 卡片内容：标题 + "使用 [x] 分音符填充" + 提示 + 确定/取消
fn dialog_card(value: &str, t: &'static lumino_extras::i18n::MainTranslations) -> Element<'static> {
    let input_style = |theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(palette.background.weak.color.into()),
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: palette.background.strong.color,
            },
            ..Default::default()
        }
    };

    let input_row = row![
        text(t.fill_division_prefix).size(14),
        Space::new().width(8),
        container(
            text_input("", value)
                .on_input(|s| AppMessage::FillDivision(Action::ValueChanged(s)))
                .padding([6, 10])
                .width(Length::Fixed(80.0))
        )
        .width(Length::Fixed(80.0))
        .style(input_style),
        Space::new().width(8),
        text(t.fill_division_suffix).size(14),
    ]
    .align_y(Alignment::Center);

    let content = column![
        text(t.fill_division_title).size(16),
        Space::new().height(16),
        input_row,
        Space::new().height(10),
        text(t.fill_division_hint).size(12),
        Space::new().height(20),
        row![
            button(text(t.precision_cancel).size(14))
                .padding([8, 24])
                .style(secondary_button_style)
                .on_press(AppMessage::FillDivision(Action::CloseDialog)),
            Space::new().width(12),
            button(text(t.precision_ok).size(14))
                .padding([8, 24])
                .style(primary_button_style)
                .on_press(AppMessage::FillDivision(Action::Confirm)),
        ]
        .spacing(0)
        .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .align_x(Alignment::Center);

    container(content)
        .width(Length::Fixed(CARD_WIDTH))
        .padding(24)
        .style(|theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(
                theme.extended_palette().background.base.color,
            )),
            border: iced_core::Border::default().rounded(8),
            ..Default::default()
        })
        .into()
}

/// 次级按钮样式（取消）
fn secondary_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let bg = match status {
        button::Status::Hovered => palette.background.strong.color,
        _ => palette.background.weak.color,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: palette.background.neutral.text,
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

/// 主按钮样式（确定）
fn primary_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let bg = match status {
        button::Status::Hovered => palette.primary.strong.color,
        _ => palette.primary.base.color,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: palette.primary.base.text,
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::root_state::FillDivisionDialogState;

    #[test]
    fn test_view_builds_element() {
        let state = FillDivisionDialogState::new();
        let _ = view_fill_division_dialog(&state, Language::ZhCn);
        let _ = view_fill_division_dialog(&state, Language::EnUs);
    }

    #[test]
    fn test_card_builds_with_value() {
        let t = main_translations(Language::ZhCn);
        let _ = dialog_card("16", t);
    }
}
