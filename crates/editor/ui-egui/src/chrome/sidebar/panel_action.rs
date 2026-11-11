//! 左侧栏面板（音轨列表）的动作通道。
//!
//! 面板绘制在 `track_list.rs`；这里只承载「点到了什么」，由生产侧映射成
//! `Message::Sidebar(sidebar::Event::*)`。一帧只保留一个动作（与其它通道一致）。

/// 音轨列表上的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPanelAction {
    /// 选中音轨（携带生产音轨 ID，不是列表下标）。
    SelectTrack(usize),
    /// 添加音轨。
    AddTrack,
}

/// 动作通道 ID。
const ACTION_ID: &str = "lumino.sidebar-panel.action";

/// 记录本帧动作（一帧只保留第一个，避免「点行 + 点添加」互相覆盖）。
pub fn push_panel_action(ctx: &egui::Context, action: SidebarPanelAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<SidebarPanelAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_panel_action(ctx: &egui::Context) -> Option<SidebarPanelAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<SidebarPanelAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}
