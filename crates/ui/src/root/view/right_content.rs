//! 右侧内容区域视图
//!
//! 提供工具栏溢出菜单覆盖层和 responsive 包装，使工具栏能感知实际可用宽度。

use iced_core::Size;
use iced_widget::{Stack, responsive};

use crate::root::Root;
use crate::toolbar::overflow;
use crate::{Element};

/// 将右侧内容包装在 responsive 中，并在工具栏溢出菜单打开时叠加覆盖层。
///
/// `has_selection` 用于控制溢出菜单中依赖选中的工具按钮是否可用。
/// `build_content` 接收实际可用宽度，负责构建工具栏与下方内容。
pub fn wrap_right_content<'a>(
    root: &'a Root,
    has_selection: bool,
    build_content: impl Fn(f32) -> Element<'a> + 'a,
) -> Element<'a> {
    responsive(move |size: Size| {
        let content = build_content(size.width);
        with_toolbar_overlay(root, content, size.width, has_selection)
    })
    .into()
}

/// 叠加工具栏溢出菜单（点击外部区域关闭）
fn with_toolbar_overlay<'a>(
    root: &'a Root,
    content: Element<'a>,
    available_width: f32,
    has_selection: bool,
) -> Element<'a> {
    if !root.toolbar.overflow_menu_open {
        return content;
    }

    let (_, hidden) = root.toolbar.compute_overflow_groups(available_width);
    if hidden.is_empty() {
        return content;
    }

    let menu = root.toolbar.render_overflow_menu(
        &hidden,
        has_selection,
        root.settings.language,
    );
    let menu_overlay = overflow::positioned_overflow_menu(menu, root.toolbar.height());

    Stack::new()
        .push(content)
        .push(overflow::background_close_overlay())
        .push(menu_overlay)
        .into()
}

/// 关闭背景：点击菜单外部区域关闭
///
/// 作为 Stack 的底层，覆盖整个父区域，点击时关闭菜单。
#[allow(dead_code)]
pub fn background_close_overlay<'a>() -> Element<'a> {
    overflow::background_close_overlay()
}
