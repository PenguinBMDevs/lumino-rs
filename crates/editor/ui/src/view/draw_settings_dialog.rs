//! 「音符画设置」对话框视图（独立 OS 窗口）
//!
//! 音符画悬浮工具条**右端齿轮按钮**的落点：把各绘制工具的公共参数聚合到一处，
//! 用户不必先激活某个工具、再"再次点击该条目"才能摸到设置。
//!
//! 与 `brush_settings_dialog.rs` 同族（主题底色铺满 + 标题 + 内容 + 底部按钮行），
//! 区别只在本轮内容仍是**占位**：设置项逐个接入时替换中段，标题与底部按钮行不动。
//!
//! 之所以做成独立窗口而不是主窗口内的悬浮小面板：
//! - 面板宽度固定 248，容纳不下成组的设置项（下拉 / 输入 / 列表），要么溢出、
//!   要么被迫做成滚动小窗，反而比独立窗口更难用；
//! - 独立窗口有系统标题栏、可拖动、可独立缩放，与其余设置类入口（画刷绘制行为）一致。

use iced_core::Length;
use iced_widget::{button, column, container, space, text};
use lumino_extras::i18n::{Language, main_translations};
use lumino_ui_core::color::contrast_text_color;

use crate::Element;
use crate::message::{DrawSettingsAction, Message};

/// 渲染「音符画设置」对话框
pub fn view_draw_settings_dialog(theme: &iced_core::Theme, language: Language) -> Element<'static> {
    let t = main_translations(language);

    // 标题：与窗口标题、悬浮条齿轮 tooltip 同文案（`tool_panel_settings`），
    // 三处一致，用户不会怀疑"点进来的和点的不像一个东西"。
    let title = text(t.tool_panel_settings).size(18);

    // 占位说明：**明确告知开发中**，不摆点了没反应的假控件（假控件 = 静默失效）。
    // `width(Fill)` 让长句在窗口宽度内自动折行。
    let body = text(t.draw_settings_placeholder)
        .size(13)
        .width(Length::Fill);

    // 底部按钮行：仅「关闭」。占位阶段没有可保存的改动，
    // 摆「保存」只会让用户以为存了什么（与画刷对话框的 保存/取消 不同——那边有草稿）。
    let buttons = column![
        button(text(t.file_close).size(14))
            .on_press(Message::DrawSettings(DrawSettingsAction::CloseDialog))
            .padding([6, 16]),
    ]
    .align_x(iced_core::Alignment::End);

    let content = column![
        title,
        space().height(12),
        body,
        space().height(Length::Fill),
        buttons,
    ]
    .spacing(4)
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill);

    // 对话框背景为当前主题底色，文字颜色按实际背景亮度计算，
    // 保证亮/暗模式下都可读（对齐代码库 contrast_text_color 规则）。
    let dialog_bg = theme.palette().background;
    let dialog_text_color = contrast_text_color(dialog_bg);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced_core::Theme| container::Style {
            background: Some(iced_core::Background::Color(dialog_bg)),
            text_color: Some(dialog_text_color),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    /// 渲染冒烟测试：中英双语 + 明暗主题下均能构建，不 panic。
    #[test]
    fn test_view_draw_settings_dialog_builds() {
        let _ = view_draw_settings_dialog(&Theme::Dark, Language::ZhCn);
        let _ = view_draw_settings_dialog(&Theme::Light, Language::EnUs);
    }

    /// 文案非空护栏：标题 / 占位说明 / 关闭按钮在两种语言下都必须有字。
    ///
    /// 空标题 = 一个只有边框的空白对话框，而这类"静默空白"在 UI 上极难定位
    /// （既不报错也不崩溃）。把三个翻译键逐条钉住，翻译表漏填当场失败。
    #[test]
    fn test_draw_settings_dialog_texts_are_non_empty() {
        for lang in [Language::ZhCn, Language::EnUs] {
            let t = main_translations(lang);
            assert!(
                !t.tool_panel_settings.trim().is_empty(),
                "{lang:?}：对话框标题（tool_panel_settings）不得为空"
            );
            assert!(
                !t.draw_settings_placeholder.trim().is_empty(),
                "{lang:?}：占位说明（draw_settings_placeholder）不得为空"
            );
            assert!(
                !t.file_close.trim().is_empty(),
                "{lang:?}：关闭按钮（file_close）不得为空"
            );
        }
    }
}
