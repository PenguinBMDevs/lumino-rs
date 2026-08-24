//! 右侧内容区域视图
//!
//! 提供工具栏溢出菜单覆盖层和 responsive 包装，使工具栏能感知实际可用宽度。
//! 溢出菜单框体为矩形，自动计算边长贴近正方形，匹配按钮排列方式；
//! 背景色贴近工具栏背景色。

use iced_core::{Color, Size};
use iced_widget::{Stack, responsive};

use crate::Element;
use crate::root::Root;
use crate::toolbar::{overflow, tool_panel};

/// 将右侧内容包装在 responsive 中，并在工具栏溢出菜单打开时叠加覆盖层。
///
/// `has_selection` 用于控制溢出菜单中依赖选中的工具按钮是否可用。
/// `build_content` 接收实际可用宽度，负责构建工具栏与下方内容。
pub fn wrap_right_content<'a>(
    root: &'a Root,
    has_selection: bool,
    arrangement_mode: bool,
    build_content: impl Fn(f32) -> Element<'a> + 'a,
) -> Element<'a> {
    responsive(move |size: Size| {
        let content = build_content(size.width);
        with_toolbar_overlay(root, content, size.width, has_selection, arrangement_mode)
    })
    .into()
}

/// 叠加工具栏溢出菜单（点击外部区域关闭）
fn with_toolbar_overlay<'a>(
    root: &'a Root,
    content: Element<'a>,
    available_width: f32,
    has_selection: bool,
    arrangement_mode: bool,
) -> Element<'a> {
    if !root.toolbar.overflow_menu_open && !root.toolbar.tool_panel_open {
        return content;
    }

    // 计算面板背景色：贴近工具栏背景色
    let palette = root.window.theme.extended_palette();
    let toolbar_bg = palette.background.weakest.color;
    // 稍微加深一点，使菜单框更显眼
    let panel_background = Color::from_rgba(
        toolbar_bg.r * 0.9,
        toolbar_bg.g * 0.9,
        toolbar_bg.b * 0.9,
        toolbar_bg.a,
    );

    let mut stack = Stack::new().push(content);

    // 工具栏「更多工具」溢出菜单
    if root.toolbar.overflow_menu_open {
        let (_, hidden) = root
            .toolbar
            .compute_overflow_groups(available_width, arrangement_mode);
        if !hidden.is_empty() {
            let menu = root.toolbar.render_overflow_menu(
                &hidden,
                has_selection,
                root.settings.display.language,
                panel_background,
                &root.window.theme,
                arrangement_mode,
            );
            let menu_overlay = overflow::positioned_overflow_menu(menu, root.toolbar.height());
            stack = stack
                .push(overflow::background_close_overlay())
                .push(menu_overlay);
        }
    }

    // 绘制工具选择面板（颜料桶右侧小三角触发）
    if root.toolbar.tool_panel_open {
        let menu = tool_panel::render_tool_panel(
            root.settings.display.language,
            panel_background,
            &root.window.theme,
        );
        let menu_overlay = overflow::positioned_overflow_menu(menu, root.toolbar.height());
        stack = stack
            .push(overflow::background_close_overlay())
            .push(menu_overlay);
    }

    stack.into()
}

/// 关闭背景：点击菜单外部区域关闭
///
/// 作为 Stack 的底层，覆盖整个父区域，点击时关闭菜单。
#[allow(dead_code)]
pub fn background_close_overlay<'a>() -> Element<'a> {
    overflow::background_close_overlay()
}
