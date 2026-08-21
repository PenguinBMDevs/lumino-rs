//! 视频剪辑预览卡片构建（几何结构与渲染内容分离）
//!
//! 几何结构固定为三层，杜绝拉伸：
//! 外层包装（`Fill` 宽 × `Fixed` 高 + `align_x` 水平居中）
//! → 黑底 16:9 盒（`Fixed` × `Fixed`）
//! → 内容（`Fill` 铺满盒内，显示区比例恒等于盒子比例）。
//!
//! ⚠️ 禁止使用 `center_x(Length::Fill)` / `center_y(Length::Fill)` 做"居中"：
//! iced 源码中它们等价于 `self.width(width)` / `self.height(height)`，
//! 会把已设置的 `Fixed` 尺寸**覆盖**为 `Fill`（本次 16:9 回归的根因之一）。

use iced_core::{Color, Element, Length, alignment};

/// 构建 16:9 预览卡片（居中包装 + 黑底 Fixed 盒 + 任意内容）
///
/// * `preview_w` / `preview_h`：来自 [`crate::view::video_clip::layout`] 的 16:9 尺寸，
///   同时是离屏纹理的存储尺寸（存储与显示同源，比例必然一致）。
/// * `content`：铺满黑盒的内容元素（shader 图元或占位文本）。
/// * 返回的外层包装 `Fill` 宽 × `Fixed(preview_h)` 高，在父 `Column` 中
///   不与其它 `Fill` 子项抢高度（高度精确匹配预留计算）。
pub fn preview_card<'a, Message, Renderer>(
    preview_w: f32,
    preview_h: f32,
    border_color: Color,
    base_color: Color,
    content: impl Into<Element<'a, Message, iced_core::Theme, Renderer>>,
) -> Element<'a, Message, iced_core::Theme, Renderer>
where
    Message: 'a,
    Renderer: iced_core::Renderer + 'a,
{
    // 黑底 16:9 盒：Fixed × Fixed，内容 Fill 恰好铺满，不放大不缩小
    let black_box = iced_widget::container(content)
        .width(Length::Fixed(preview_w))
        .height(Length::Fixed(preview_h))
        .style(move |_theme| iced_widget::container::Style {
            background: Some(Color::BLACK.into()),
            border: iced_core::Border {
                color: border_color,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

    // 居中包装：Fill 宽 × Fixed 高；仅 align_x（不改宽度），黑盒水平居中、垂直恰好占满
    iced_widget::container(black_box)
        .width(Length::Fill)
        .height(Length::Fixed(preview_h))
        .align_x(alignment::Horizontal::Center)
        .style(move |_theme| iced_widget::container::Style {
            background: Some(base_color.into()),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::video_clip::layout;
    use iced_core::Size;
    use iced_core::layout::Limits;
    use iced_core::widget::Tree;

    /// 测试元素类型：`()` 即 iced 内置 headless 渲染器（debug 构建下实现全部所需 trait）
    type TestElement<'a> = Element<'a, (), iced_core::Theme, ()>;

    /// 构建与 `panels.rs::view_renderer_panel` 相同结构的测试面板：
    /// `row[左轨道面板(220), column[header(40), preview_card, timeline(200), settings(80)]]`
    /// （spacing 12 / padding 12 与生产代码一致）
    fn build_panel(main: Size) -> TestElement<'static> {
        let (pw, ph) = layout::renderer_panel_preview_size(main);

        let header = iced_widget::space().height(Length::Fixed(layout::HEADER_HEIGHT));
        let timeline = iced_widget::space().height(Length::Fixed(layout::TIMELINE_HEIGHT));
        let settings = iced_widget::space().height(Length::Fixed(layout::SETTINGS_HEIGHT));

        // 内容用 Fill×Fill 的 space 模拟 shader 图元（布局行为一致：铺满黑盒）
        let card = preview_card::<(), ()>(
            pw,
            ph,
            Color::WHITE,
            Color::BLACK,
            iced_widget::space()
                .width(Length::Fill)
                .height(Length::Fill),
        );

        let right_col = iced_widget::column![header, card, timeline, settings,]
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(layout::ROW_SPACING);

        let left = iced_widget::container(iced_widget::space())
            .width(Length::Fixed(layout::LEFT_RESERVED))
            .height(Length::Fill);

        iced_widget::row![left, right_col]
            .spacing(layout::ROW_SPACING)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(layout::PANEL_PADDING)
            .into()
    }

    /// 对真实 widget 树执行 iced headless 布局，返回
    /// （预览包装器, 黑盒, 内容）三个节点的实际 bounds（绝对坐标）
    fn measure(
        main: Size,
    ) -> (
        iced_core::Rectangle,
        iced_core::Rectangle,
        iced_core::Rectangle,
    ) {
        let mut el = build_panel(main);
        let mut tree = Tree::new(&el);
        let limits = Limits::new(Size::ZERO, main);
        let root = el.as_widget_mut().layout(&mut tree, &(), &limits);

        // 用 Layout 遍历（逐层累加 offset → 绝对坐标）；裸 Node::bounds 是相对父节点的
        let root_layout = iced_core::Layout::new(&root);
        // 结构索引：root.children = [left, right_col]；
        // right_col.children = [header, wrapper, timeline, settings]；
        // wrapper.children = [black_box]；black_box.children = [content]
        let right_col = root_layout.children().nth(1).expect("right col 节点缺失");
        let wrapper = right_col.children().nth(1).expect("预览包装器节点缺失");
        let black_box = wrapper.children().next().expect("黑盒节点缺失");
        let content = black_box.children().next().expect("内容节点缺失");
        (wrapper.bounds(), black_box.bounds(), content.bounds())
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.5
    }

    #[test]
    fn test_ui_preview_window_is_strict_16_9() {
        // 多组主窗口尺寸下，UI 中实际布局出的预览窗口（黑盒）必须严格 16:9
        let cases = [
            (1280.0, 800.0),
            (1920.0, 1080.0),
            (1600.0, 900.0),
            (1024.0, 768.0),
            (800.0, 600.0),
        ];
        for (w, h) in cases {
            let (_wrapper, black, _content) = measure(Size::new(w, h));
            let ratio = black.width / black.height;
            assert!(
                (ratio - 16.0 / 9.0).abs() < 0.01,
                "主区域 {w}x{h}：UI 预览窗口实际布局 {}x{}，比例 {ratio:.4} ≠ 16/9",
                black.width,
                black.height
            );
        }
    }

    #[test]
    fn test_ui_preview_matches_computed_storage_size() {
        // UI 布局出的黑盒尺寸必须与纯函数计算的存储尺寸一致（存储=显示，同源不漂移）
        let cases = [(1280.0, 800.0), (1920.0, 1080.0), (1024.0, 768.0)];
        for (w, h) in cases {
            let (pw, ph) = layout::renderer_panel_preview_size(Size::new(w, h));
            let (_wrapper, black, _content) = measure(Size::new(w, h));
            assert!(
                approx(black.width, pw) && approx(black.height, ph),
                "主区域 {w}x{h}：UI 黑盒 {}x{} ≠ 计算存储 {pw}x{ph}",
                black.width,
                black.height
            );
        }
    }

    #[test]
    fn test_content_fills_black_box_without_distortion() {
        // 内容（模拟 shader 显示区）必须恰好铺满黑盒内边界——比例与盒子一致即无拉伸
        let cases = [(1280.0, 800.0), (1600.0, 900.0), (800.0, 600.0)];
        for (w, h) in cases {
            let (_wrapper, black, content) = measure(Size::new(w, h));
            assert!(
                approx(content.width, black.width) && approx(content.height, black.height),
                "主区域 {w}x{h}：内容 {}x{} 未铺满黑盒 {}x{}",
                content.width,
                content.height,
                black.width,
                black.height
            );
            let ratio = content.width / content.height;
            assert!(
                (ratio - 16.0 / 9.0).abs() < 0.01,
                "主区域 {w}x{h}：内容显示比例 {ratio:.4} ≠ 16/9（画面被拉伸）"
            );
        }
    }

    #[test]
    fn test_black_box_contained_in_wrapper_no_clipping() {
        // 黑盒必须完整落在包装器内（不被裁切、不溢出到时间轴区域）
        let cases = [(1280.0, 800.0), (1920.0, 1080.0), (800.0, 600.0)];
        for (w, h) in cases {
            let (wrapper, black, _content) = measure(Size::new(w, h));
            assert!(
                black.y >= wrapper.y - 0.01
                    && black.y + black.height <= wrapper.y + wrapper.height + 0.01,
                "主区域 {w}x{h}：黑盒 y[{:.1},{:.1}] 溢出包装器 y[{:.1},{:.1}]（被裁切）",
                black.y,
                black.y + black.height,
                wrapper.y,
                wrapper.y + wrapper.height
            );
            assert!(black.x >= wrapper.x - 0.01);
            assert!(black.x + black.width <= wrapper.x + wrapper.width + 0.01);
        }
    }
}
