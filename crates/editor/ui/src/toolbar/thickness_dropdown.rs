//! 「粗细」悬浮面板（悬浮条右端「粗细」按钮触发；**向上弹出**）
//!
//! 与画刷 / 形状 / 分音符填充下拉同属「工具栏弹出的小面板」家族：锚定在触发它的
//! 图标按钮**上方**（`CurveToolGroup` 同一套悬浮层；顶部空间不足才自动下翻）、
//! 工具栏派生的深色背景、圆角 8。
//!
//! 内容：**一根滑杆** + 当前值 + 一行说明。
//! - 量程 `1..=100`、步进（精度阈值）`1`，两端常量取自 [`BrushConfig`]——
//!   滑杆右端与 `set_thickness` 的 clamp 同源，改一处即同步，不会出现
//!   "滑到头却写不进去"的静默截断；
//! - 值是**全局**的：曲线 / 形状的描边与画刷笔画共用它（纵向 key 层数），
//!   画布上"正在画的线"的线宽 = 层数 × 单 key 高（见
//!   `lumino_ui_editor::Editor::draw_line_width_px`）。
//!
//! 视觉说明：面板底色由工具栏底色派生，亮色模式下可能仍偏暗，故文字颜色按
//! 实际背景亮度计算（`contrast_text_color`），避免亮色模式下黑字落在暗面板上不可见。

use iced_core::{Alignment, Background, Border, Color, Length};
use iced_widget::{column, container, row, space, text};
use lumino_core::BrushConfig;
use lumino_extras::i18n::{Language, main_translations};
use lumino_ui_core::color::contrast_text_color;

use crate::Element;
use crate::toolbar::Event;

/// 滑杆宽度（像素）：面板宽 248 − 左右内边距 12×2 = 224，取 200 留出手感余量。
const SLIDER_WIDTH: f32 = 200.0;

/// 渲染「粗细」悬浮面板。
///
/// - `thickness`：全局粗细当前值（`Toolbar::brush.thickness`，物理载体见 `BrushConfig`）；
/// - `panel_background`：面板背景色（由调用方据工具栏背景计算，贴近工具栏配色）；
/// - `theme`：当前主题（用于边框 / 滑杆配色）。
pub(crate) fn render_thickness_dropdown<'a>(
    thickness: u8,
    language: Language,
    panel_background: Color,
    theme: &'a iced_core::Theme,
) -> Element<'a> {
    let t = main_translations(language);
    let palette = theme.extended_palette();
    let panel_text_color = contrast_text_color(panel_background);

    // 夹到合法量程：防上游写入越界值后滑杆与显示各说各话（显示的必须是能存下来的）
    let value = thickness.clamp(BrushConfig::MIN_THICKNESS, BrushConfig::MAX_THICKNESS);

    // 顶部信息行：标题 + 当前值 + 量程（量程由常量算出，不硬编码数字）
    let info_row = row![
        text(t.tool_thickness).size(14),
        space().width(8),
        text(format!(
            "{value} {} ({} - {})",
            t.draw_thickness_unit,
            BrushConfig::MIN_THICKNESS,
            BrushConfig::MAX_THICKNESS
        ))
        .size(14),
    ]
    .align_y(Alignment::Center);

    // 滑杆：量程 / 步进与 `BrushConfig` 同源（步进 = 精度阈值 1）
    let slider = iced_widget::slider(
        BrushConfig::MIN_THICKNESS..=BrushConfig::MAX_THICKNESS,
        value,
        Event::draw_thickness_changed,
    )
    .step(1_u8)
    .width(Length::Fixed(SLIDER_WIDTH));

    let hint = text(t.draw_thickness_hint)
        .size(11)
        .width(Length::Fixed(SLIDER_WIDTH));

    container(
        column![info_row, slider, hint]
            .spacing(8)
            .align_x(Alignment::Start)
            .padding(12),
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

    /// 渲染冒烟测试：两种语言 + 边界值（下限 / 上限 / 越界）下均能构建，不 panic。
    #[test]
    fn test_render_thickness_dropdown_builds() {
        let theme = crate::Theme::Dark;
        let bg = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
        for lang in [Language::ZhCn, Language::EnUs] {
            for value in [
                0,
                BrushConfig::MIN_THICKNESS,
                42,
                BrushConfig::MAX_THICKNESS,
            ] {
                let _ = render_thickness_dropdown(value, lang, bg, &theme);
            }
        }
    }

    /// 量程契约：滑杆两端就是 `BrushConfig` 的写入量程（clamp 与滑杆同源）。
    ///
    /// 这条钉死"滑到头却写不进去"的那类静默截断：滑杆右端 100 必须能被
    /// `set_thickness` 原样写入（上限从 20 放宽到 100 就是这一步）。
    #[test]
    fn test_slider_range_matches_clamp_range() {
        let mut config = BrushConfig::new();
        config.set_thickness(BrushConfig::MAX_THICKNESS);
        assert_eq!(
            config.thickness,
            BrushConfig::MAX_THICKNESS,
            "滑杆右端值必须能原样写入（clamp 不得截掉）"
        );
        config.set_thickness(0);
        assert_eq!(
            config.thickness,
            BrushConfig::MIN_THICKNESS,
            "越界下限应夹到最小粗细"
        );
        config.set_thickness(u8::MAX);
        assert_eq!(
            config.thickness,
            BrushConfig::MAX_THICKNESS,
            "越界上限应夹到最大粗细"
        );
    }
}
