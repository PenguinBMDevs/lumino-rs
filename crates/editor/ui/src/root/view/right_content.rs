//! 右侧内容区域视图
//!
//! 只保留 `responsive` 包装：工具栏溢出菜单、音轨列表菜单、悬浮工具条等
//! 覆盖层全部由 egui 绘制并接管点击（`lumino-ui-egui::chrome`），
//! iced 侧不再叠第二份。

use iced_core::Size;
use iced_widget::responsive;

use crate::Element;
use crate::root::Root;

/// 将右侧内容包装在 `responsive` 中（构建回调接收实际可用宽度）。
pub fn wrap_right_content<'a>(
    _root: &'a Root,
    _has_selection: bool,
    _arrangement_mode: bool,
    build_content: impl Fn(f32) -> Element<'a> + 'a,
) -> Element<'a> {
    responsive(move |size: Size| build_content(size.width)).into()
}
