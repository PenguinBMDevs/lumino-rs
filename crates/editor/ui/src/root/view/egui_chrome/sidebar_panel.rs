//! 左侧栏音轨列表动作 → 生产消息。
//!
//! `lumino-ui-egui` 只回传「点了哪条音轨 / 点了添加」，这里翻译成
//! `Message::Sidebar(sidebar::Event::*)`，与 iced 面板走同一批构造器。

use lumino_ui_egui::chrome::SidebarPanelAction;

use crate::Message;
use crate::root::Root;

impl Root {
    /// 音轨列表动作 → 生产 `Message`。
    pub(crate) fn sidebar_panel_action_message(&self, action: SidebarPanelAction) -> Message {
        use crate::sidebar::Event;
        match action {
            SidebarPanelAction::SelectTrack(id) => Event::track_selected(id),
            SidebarPanelAction::AddTrack => Event::add_track(),
        }
    }
}
