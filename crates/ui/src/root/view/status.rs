//! 状态栏视图渲染函数
//!
//! 状态栏的渲染委托给 statusbar.view()。

use crate::root::{Element, Root};

impl Root {
    /// 渲染状态栏（性能面板已交由 Stack 浮动层处理）
    pub(super) fn view_status_section(&self) -> Element<'_> {
        self.statusbar.view(self.settings.language)
    }
}
