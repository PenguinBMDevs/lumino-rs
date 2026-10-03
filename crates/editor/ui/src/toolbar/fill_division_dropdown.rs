//! 颜料桶「分音符填充」下拉（Ctrl+单击颜料桶触发）
//!
//! 与画刷 / 形状下拉同属「工具栏 Ctrl+弹出的小面板」家族：贴图标上方的紧凑面板、
//! 工具栏派生的深色背景、圆角 8（对齐 `brush_dropdown.rs` / `shape_dropdown.rs`）。
//!
//! 触发路径（两条共用同一状态 `state.fill_division_dialog`）：
//! - 音符画工具箱里 Ctrl+单击「颜料桶」条目（`ToolPanelItemCtrlSelected`）；
//! - 画布上 Ctrl+单击（编辑器置请求位 → `open_fill_division_dialog`）。
//!
//! 交互：输入 x（**任意数字**，留空 = 整块填充）→ [确定] 写入切分档位；
//! [取消] / 点击面板内空白 → 关闭，不改档位。
//!
//! 视觉说明：面板底色由工具栏底色派生，亮色模式下可能仍偏暗，故文字颜色按
//! 实际背景亮度计算（`contrast_text_color`），避免亮色模式下黑字落在暗面板上不可见。

use iced_core::{Alignment, Background, Border, Color, Length};
use iced_widget::{button, column, container, row, space, text, text_input};
use lumino_ui_core::color::contrast_text_color;

use crate::Element;
use crate::message::{FillDivisionAction, Message};
use lumino_extras::i18n::{Language, main_translations};

/// 输入框宽度（X 向）
const INPUT_WIDTH: f32 = 72.0;

/// 渲染颜料桶「分音符填充」下拉。
///
/// - `value`：输入框当前文本（来自 `state.fill_division_dialog.value`）；
/// - `panel_background`：面板背景色（由调用方据工具栏背景计算，贴近工具栏配色）；
/// - `theme`：当前主题（用于边框 / 输入框配色）。
pub(crate) fn render_fill_division_dropdown<'a>(
    value: &'a str,
    language: Language,
    panel_background: Color,
    theme: &'a iced_core::Theme,
) -> Element<'a> {
    let t = main_translations(language);
    let palette = theme.extended_palette();
    let panel_text_color = contrast_text_color(panel_background);

    // 输入行：使用 [ x ] 分音符填充
    let input_row = row![
        text(t.fill_division_prefix).size(14),
        space().width(6),
        text_input("", value)
            .on_input(|s| Message::FillDivision(FillDivisionAction::ValueChanged(s)))
            .padding([4, 8])
            .width(Length::Fixed(INPUT_WIDTH)),
        space().width(6),
        text(t.fill_division_suffix).size(14),
    ]
    .align_y(Alignment::Center);

    // 按钮行：取消 / 确定（与面板同底色，风格对齐画刷 / 形状下拉）
    let buttons = row![
        button(text(t.precision_cancel).size(13))
            .padding([4, 14])
            .on_press(Message::FillDivision(FillDivisionAction::CloseDialog)),
        space().width(8),
        button(text(t.precision_ok).size(13))
            .padding([4, 14])
            .on_press(Message::FillDivision(FillDivisionAction::Confirm)),
    ]
    .align_y(Alignment::Center);

    container(
        column![
            input_row,
            space().height(4),
            text(t.fill_division_hint).size(11),
            space().height(8),
            buttons,
        ]
        .align_x(Alignment::Center)
        .padding(10),
    )
    .style(move |_theme: &iced_core::Theme| container::Style {
        background: Some(Background::Color(panel_background)),
        text_color: Some(panel_text_color),
        border: Border {
            width: 1.0,
            color: palette.background.strong.color,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 渲染冒烟测试：两种语言下均能构建，不 panic。
    #[test]
    fn test_render_fill_division_dropdown_builds() {
        let theme = crate::Theme::Dark;
        let bg = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
        let _ = render_fill_division_dropdown("", Language::ZhCn, bg, &theme);
        let _ = render_fill_division_dropdown("16", Language::EnUs, bg, &theme);
    }
}
