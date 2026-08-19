//! 右侧栏钢琴瀑布流预览面板
//!
//! 以「离屏 wgpu 渲染 → iced `image::Handle`」的方式绘制：
//! - 底部标准钢琴键盘（键数跟随全局 `enable_256key`：开启 256 键，否则 128 键）；
//! - 上方下落式音符瀑布流，复用渲染线程活体 GPU 实例缓冲（禁止第二份拷贝），
//!   配色与卷帘洋葱皮逐位一致，主音轨音符显示为蓝色，滚动与卷帘 X 缩放/滚动同步。
//! 纹理由 Host 在 GPU 上下文持有者处渲染并缓存，面板仅负责展示。

pub(crate) mod key_layout;
pub(crate) mod keyboard_renderer;

use iced_core::Length;
use iced_widget::{Column, container, text};

use lumino_extras::i18n::{Language, main_translations};

use crate::right_sidebar::core::{RESIZE_HANDLE_WIDTH, RightSidebar};
use crate::{Element, Theme, window};

use self::keyboard_renderer::{KEY_HEIGHT_RATIO, MAX_KEY_HEIGHT, MIN_KEY_HEIGHT, PANEL_PADDING};

/// 渲染钢琴瀑布流预览面板内容（标题 + 说明 + 底部键盘图像）
pub(super) fn panel<'a>(
    right_sidebar: &'a RightSidebar,
    language: Language,
    _window: &'a window::Window,
) -> Element<'a> {
    let t = main_translations(language);
    let state = &right_sidebar.piano_waterfall;

    let top_col = Column::new()
        .spacing(8)
        .padding(PANEL_PADDING)
        .width(Length::Fill)
        .push(panel_header(format!("{}预览", t.piano_waterfall), _window));

    // 瀑布流图像占满面板剩余高度（音符随卷帘滚动下落，底部为钢琴键盘落点）
    let bottom_section: crate::Element<'a> = if let Some(handle) = &state.handle {
        iced_widget::image::Image::<iced_core::image::Handle>::new(handle.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        text("（键盘渲染中或面板不可见）")
            .size(11)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.strong.text),
            })
            .into()
    };

    let content_col = Column::new()
        .width(Length::Fill)
        .push(top_col)
        .push(bottom_section);

    let content = container(content_col)
        .width(Length::Fixed(
            right_sidebar.panel_width - RESIZE_HANDLE_WIDTH,
        ))
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weakest.color)
        });

    content.into()
}

/// 面板标题文本（跟随主题：暗色白、亮色黑）
fn panel_header<'a>(title: String, _window: &'a window::Window) -> Element<'a> {
    text(title)
        .size(14)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().background.neutral.text),
        })
        .into()
}

/// 由面板内容宽度推导键盘渲染尺寸（宽度变化时高度按比例联动）
#[allow(dead_code)]
pub(crate) fn keyboard_size(content_width: f32) -> (u32, u32) {
    let w = (content_width - PANEL_PADDING * 2.0).max(1.0);
    let h = (w * KEY_HEIGHT_RATIO).clamp(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
    (w as u32, h as u32)
}
