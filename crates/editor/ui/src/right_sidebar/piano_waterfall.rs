//! 右侧栏钢琴瀑布流预览面板
//!
//! 入口阶段：仅渲染面板外壳（标题 + 占位说明）。瀑布流预览的实际渲染
//! （音符随音频下落的时间轴可视化）将在后续迭代中接入数据层。

use iced_core::{Length};
use iced_widget::{Column, container, scrollable, text};
use lumino_extras::i18n::{Language, main_translations};

use crate::right_sidebar::core::{RESIZE_HANDLE_WIDTH, RightSidebar};
use crate::{Element, Theme, window};

/// 渲染钢琴瀑布流预览面板内容（标题 + 占位说明）
pub(super) fn panel<'a>(
    right_sidebar: &'a RightSidebar,
    language: Language,
    _window: &'a window::Window,
) -> Element<'a> {
    let t = main_translations(language);

    let content_col = Column::new()
        .spacing(8)
        .padding(8)
        .width(Length::Fill)
        .push(panel_header(format!("{}预览", t.piano_waterfall), _window))
        .push(
            text("钢琴瀑布流预览即将在此处呈现：音符将随播放进度自上而下流动，\
                形成类似瀑布的可视化。当前为功能入口占位，数据接入待后续迭代。")
                .size(12)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().background.strong.text),
                }),
        );

    let content = container(scrollable(content_col).height(Length::Fill))
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
