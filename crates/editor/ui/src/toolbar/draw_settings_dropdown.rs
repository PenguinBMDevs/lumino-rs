//! 「音符画设置」总面板（悬浮条**右端齿轮按钮**触发）
//!
//! 与画刷 / 形状 / 分音符填充三块**工具自带**设置并列，但语义是**总入口**：
//! 后续把各绘制工具的公共参数（描边、画笔、形状、颜料桶等）聚合到这一处，
//! 用户不必先激活某个工具、再"再次点击该条目"才能摸到设置。
//!
//! 本轮只落 **UI 入口**：面板骨架 + 明确告知"设置项开发中"的占位文案，
//! 真正的设置项待逐个接入（避免先造一堆没有行为承接的控件）。
//!
//! 视觉与 `fill_division_dropdown.rs` / `brush_dropdown.rs` 同族：贴图标上方的紧凑面板、
//! 由工具栏底色派生的深色背景、圆角 8。面板底色由调用方传入，故文字颜色按**实际背景
//! 亮度**计算（`contrast_text_color`），亮色模式下不会出现黑字落在暗面板上不可见。

use iced_core::{Alignment, Background, Border, Color, Length};
use iced_widget::{column, container, space, text};
use lumino_ui_core::color::contrast_text_color;

use crate::Element;
use lumino_extras::i18n::{Language, main_translations};

/// 渲染「音符画设置」总面板。
///
/// - `panel_background`：面板背景色（由调用方据工具栏背景计算，贴近工具栏配色）；
/// - `theme`：当前主题（用于边框 / 圆角配色）。
pub(crate) fn render_draw_settings_panel<'a>(
    language: Language,
    panel_background: Color,
    theme: &'a iced_core::Theme,
) -> Element<'a> {
    let t = main_translations(language);
    let palette = theme.extended_palette();
    let panel_text_color = contrast_text_color(panel_background);

    // 标题：与 tooltip 同文案，保证"按钮提示"与"面板标题"一致。
    let title = text(t.tool_panel_settings).size(14);

    // 占位文案：**明确说明开发中**，不做假控件（假控件会让人以为点了没反应）。
    // `width(Fill)` 让长句在面板宽度内自动折行，不撑破背景。
    let placeholder = text(t.draw_settings_placeholder)
        .size(11)
        .width(Length::Fill);

    container(
        column![title, space().height(6), placeholder,]
            .align_x(Alignment::Start)
            .padding(10),
    )
    .width(Length::Fill)
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
    use iced_core::{Font, Pixels, Size, layout, widget};

    /// 面板固定宽度，与 `root/draw_toolbar.rs` 的 `MENU_WIDTH` 同口径。
    const MENU_WIDTH: f32 = 248.0;

    /// 渲染冒烟测试：两种语言下均能构建，不 panic。
    #[test]
    fn test_render_draw_settings_panel_builds() {
        let theme = crate::Theme::Dark;
        let bg = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
        let _ = render_draw_settings_panel(Language::ZhCn, bg, &theme);
        let _ = render_draw_settings_panel(Language::EnUs, bg, &theme);
    }

    /// 面板内容宽度护栏：占位文案（中文 / 英文）不得把面板撑出 `MENU_WIDTH`。
    ///
    /// 与分音符填充面板同一口径——面板宽度由调用方固定，内容超宽会溢出面板背景。
    #[test]
    fn test_draw_settings_panel_fits_menu_width() {
        // 文本度量需要真实渲染器；无适配器时跳过（CI 软渲染环境不保证有）。
        let Some(renderer) = headless_renderer() else {
            eprintln!("跳过：无可用 GPU 适配器（文本度量需要真实渲染器）");
            return;
        };

        for lang in [Language::ZhCn, Language::EnUs] {
            let mut element = render_draw_settings_panel(
                lang,
                Color::from_rgba(0.1, 0.1, 0.1, 1.0),
                &crate::Theme::Dark,
            );
            let mut tree = widget::Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(MENU_WIDTH, f32::INFINITY)),
            );
            let width = node.bounds().width;
            assert!(
                width <= MENU_WIDTH,
                "{lang:?} 下面板内容宽度 {width:.1} 超出 MENU_WIDTH {MENU_WIDTH}"
            );
        }
    }

    /// 构建 **headless** iced 渲染器（不需要窗口）；无可用 GPU 适配器时返回 `None`。
    ///
    /// 与 `root/draw_toolbar.rs` 的测试同源（该文件内的版本是私有的，故此处重复一份）。
    fn headless_renderer() -> Option<crate::Renderer> {
        use iced_wgpu::graphics::Shell;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        let adapter = rt
            .block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
        let (device, queue) = rt
            .block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("lumino_ui_draw_settings_test_device"),
                required_features: adapter.features() & wgpu::Features::default(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            }))
            .ok()?;
        let engine = iced_wgpu::Engine::new(
            &adapter,
            device,
            queue,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            None,
            Shell::headless(),
        );

        Some(crate::Renderer::new(
            engine,
            Font::DEFAULT,
            Pixels::from(16),
        ))
    }
}
