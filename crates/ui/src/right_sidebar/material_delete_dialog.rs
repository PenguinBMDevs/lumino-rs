//! 素材删除确认对话框（主窗口内嵌覆盖层样式）
//!
//! 由 `RightSidebar.materials.pending_delete` 驱动：右键菜单点击"删除"后
//! 在主窗口叠加全屏半透明遮罩 + 居中确认卡片（覆盖层样式，非独立窗口）。
//!
//! 交互：
//! - [删除] → `MaterialDeleteConfirmed(index)`（删除文件 + 重新扫描）
//! - [取消] / 点击遮罩 → `MaterialDeleteCancelled`
//!
//! 纯 UI 层实现：素材删除为本地文件操作，无需 runner / 独立窗口。

use iced_core::{Alignment, Color, Length};
use iced_widget::{Space, button, column, container, mouse_area, row, text};
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::RightSidebarAction;

use crate::{Element, Message, Theme};

/// 遮罩半透明黑色
const MASK_BACKGROUND: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.45);
/// 卡片宽度（X 向）
const CARD_WIDTH: f32 = 440.0;

/// 渲染素材删除确认对话框（全屏遮罩 + 居中卡片）
///
/// `name` 为素材显示名（快照；素材列表刷新后仍可正确展示）。
pub fn view(name: String, index: usize, language: Language) -> Element<'static> {
    let t = main_translations(language);

    // 全屏半透明遮罩：点击取消（关闭对话框）
    let mask: Element<'static> = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill)).style(|_theme: &Theme| {
            container::Style {
                background: Some(iced_core::Background::Color(MASK_BACKGROUND)),
                ..Default::default()
            }
        }),
    )
    .on_press(Message::RightSidebar(
        RightSidebarAction::MaterialDeleteCancelled,
    ))
    .into();

    // 居中确认卡片（mouse_area 吞掉卡片区域点击，避免触发遮罩取消）
    let card: Element<'static> = mouse_area(dialog_card(name, index, t))
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

/// 确认卡片内容：标题 + 素材名 + 危险提示 + 操作按钮
///
/// 文字与按钮行整体水平居中排布（标题/素材名/危险提示逐行居中，按钮行作为整体居中）。
fn dialog_card(
    material_name: String,
    index: usize,
    t: &'static lumino_extras::i18n::MainTranslations,
) -> Element<'static> {
    let content = column![
        // 标题
        text(t.material_delete_title)
            .size(16)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.neutral.text),
            }),
        Space::new().height(12),
        // 素材名
        text(material_name)
            .size(14)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().primary.base.color),
            }),
        Space::new().height(8),
        // 危险提示
        text(t.material_delete_warning)
            .size(13)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().danger.base.color),
            }),
        Space::new().height(20),
        // 按钮行：取消（普通）/ 删除（危险色）
        row![
            button(text(t.material_delete_cancel).size(14))
                .padding([8, 24])
                .style(secondary_button_style)
                .on_press(Message::RightSidebar(
                    RightSidebarAction::MaterialDeleteCancelled,
                )),
            Space::new().width(12),
            button(text(t.material_delete).size(14))
                .padding([8, 24])
                .style(danger_button_style)
                .on_press(Message::RightSidebar(
                    RightSidebarAction::MaterialDeleteConfirmed(index),
                )),
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

/// 危险按钮样式（确认删除）
fn danger_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let bg = match status {
        button::Status::Hovered => palette.danger.base.color,
        _ => palette.danger.weak.color,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: palette.background.base.text,
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

    #[test]
    fn test_view_builds_element() {
        let _element = view("测试素材".into(), 0, Language::ZhCn);
        let _element = view("测试素材2".into(), 3, Language::EnUs);
    }

    #[test]
    fn test_dialog_card_builds_element() {
        let t = main_translations(Language::ZhCn);
        let _element = dialog_card("测试素材".into(), 1, t);
    }
}
