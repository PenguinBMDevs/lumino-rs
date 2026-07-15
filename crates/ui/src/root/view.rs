//! Root 视图渲染子模块
//!
//! 已拆分为以下子模块：
//! - `main_view`: 主视图函数（root_view, view_main, view_arrangement 等）
//! - `overlays`: 覆盖层/弹窗视图（进度窗口、对话框）
//! - `status`: 状态栏视图

mod main_view;
mod overlays;
mod status;

use crate::Element;
use crate::root::Root;

impl Root {
    /// 渲染视图（委托给 root_view 实现）
    pub fn view(&self) -> Element<'_> {
        self.root_view()
    }
}
