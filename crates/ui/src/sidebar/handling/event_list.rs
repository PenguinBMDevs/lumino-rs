//! 事件列表滚动处理 — EventListScrolled

use crate::sidebar::core::Sidebar;

impl Sidebar {
    /// 处理事件列表滚动偏移与视口高度更新
    pub(super) fn handle_event_list_scrolled(&mut self, offset: f32, viewport_height: f32) {
        self.event_list_scroll_y = offset.max(0.0);
        self.event_list_viewport_height = viewport_height.max(0.0);
    }
}
