//! 左侧栏宽度（egui 侧同样需要，用于走带几何与画布偏移）。

use super::core::{ROUTE_BAR_WIDTH, Sidebar};

impl Sidebar {
    /// 返回侧边栏占据的宽度（像素）：路由栏 + 可见面板宽。
    pub fn width(&self) -> u32 {
        (ROUTE_BAR_WIDTH
            + if self.panel_visible {
                self.panel_width
            } else {
                0.0
            }) as u32
    }
}
