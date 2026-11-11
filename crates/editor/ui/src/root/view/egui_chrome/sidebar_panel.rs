//! 左侧栏音轨列表动作 → 生产消息。
//!
//! `lumino-ui-egui` 只回传「点了哪条音轨 / 点了添加 / 点了菜单第几项」，
//! 这里翻译成 `Message::Sidebar(sidebar::Event::*)`，与 iced 面板走同一批构造器。

use lumino_core::tr;
use lumino_message::{PanelContextMenuItem, TrackContextMenuItem};
use lumino_ui_egui::chrome::{
    SidebarMenuAction, SidebarMenuEntry, SidebarPanelAction, SidebarPanelMenuModel,
    SidebarTrackMenuModel,
};
use lumino_ui_egui::icons::Icon;

use crate::Message;
use crate::root::Root;

impl Root {
    /// 音轨列表动作 → 生产 `Message`。
    pub(crate) fn sidebar_panel_action_message(&self, action: SidebarPanelAction) -> Message {
        use crate::sidebar::Event;
        match action {
            SidebarPanelAction::SelectTrack(id) => Event::track_selected(id),
            SidebarPanelAction::AddTrack => Event::add_track(),
            SidebarPanelAction::OpenTrackMenu(id) => Event::track_context_menu_opened(id),
            SidebarPanelAction::OpenPanelMenu => Event::panel_context_menu_opened(),
            SidebarPanelAction::RenameChanged { track_id, value } => {
                Event::track_rename_changed(track_id, value.clone())
            }
            SidebarPanelAction::RenameConfirmed(id) => Event::track_rename_confirmed(id),
            SidebarPanelAction::ColorPicked { track_id, rgb } => Event::track_color_selected(
                track_id,
                iced_core::Color::from_rgb8(rgb.0, rgb.1, rgb.2),
            ),
            SidebarPanelAction::ColorReset(id) => Event::track_color_reset(id),
            SidebarPanelAction::ColorPickerClosed(id) => Event::track_color_picker_closed(id),
            SidebarPanelAction::PortPicked { track_id, value } => {
                Event::track_port_selected(track_id, value)
            }
            SidebarPanelAction::PortPickerClosed(id) => Event::track_port_picker_closed(id),
            SidebarPanelAction::ChannelPicked { track_id, value } => {
                Event::track_channel_selected(track_id, value)
            }
            SidebarPanelAction::ChannelPickerClosed(id) => Event::track_channel_picker_closed(id),
            SidebarPanelAction::ResizeStarted => Event::resize_drag_started(),
            SidebarPanelAction::ResizeEnded => Event::resize_drag_ended(),
            SidebarPanelAction::ReorderStarted(id) => Event::track_reorder_started(id),
            SidebarPanelAction::ReorderMoved { index } => Event::track_reorder_moved(
                0.0,
                crate::sidebar::TRACK_ROW_OFFSET_Y
                    + index as f32 * crate::sidebar::TRACK_ROW_HEIGHT,
            ),
            SidebarPanelAction::ReorderEnded { index } => Event::track_reorder_ended(Some(index)),
            SidebarPanelAction::ReorderCancelled => Event::track_reorder_cancelled(),
        }
    }

    /// 右键菜单动作 → 生产 `Message`；下标越界返回 `None`。
    pub(crate) fn sidebar_menu_action_message(&self, action: SidebarMenuAction) -> Option<Message> {
        use crate::sidebar::Event;
        Some(match action {
            SidebarMenuAction::TrackItem { track_id, index } => {
                let item = *TRACK_MENU_ITEMS.get(index)?;
                Event::track_context_menu_item_clicked(track_id, item)
            }
            SidebarMenuAction::CloseTrackMenu => Event::track_context_menu_closed(),
            SidebarMenuAction::PanelItem(index) => {
                let item = *PANEL_MENU_ITEMS.get(index)?;
                Event::panel_context_menu_item_clicked(item)
            }
            SidebarMenuAction::ClosePanelMenu => Event::panel_context_menu_closed(),
        })
    }
}

/// 音轨菜单条目顺序（与 iced `context_menu::panel` 完全一致）。
const TRACK_MENU_ITEMS: [TrackContextMenuItem; 5] = [
    TrackContextMenuItem::Delete,
    TrackContextMenuItem::Rename,
    TrackContextMenuItem::SetColor,
    TrackContextMenuItem::SetChannel,
    TrackContextMenuItem::SetPort,
];

/// 面板空白菜单条目顺序（与 iced `panel_context_menu::panel` 一致）。
const PANEL_MENU_ITEMS: [PanelContextMenuItem; 1] = [PanelContextMenuItem::RecoverDeletedTrack];

/// 音轨取色器模型；未打开时返回 `None`。
pub(super) fn color_picker(
    sidebar: &crate::sidebar::Sidebar,
) -> Option<lumino_ui_egui::chrome::SidebarColorPickerModel> {
    sidebar
        .color_picking_track
        .map(|track_id| lumino_ui_egui::chrome::SidebarColorPickerModel { track_id })
}

/// 端口 / 通道选择器模型；未打开时返回 `None`。
pub(super) fn numeric_picker(
    sidebar: &crate::sidebar::Sidebar,
) -> Option<lumino_ui_egui::chrome::SidebarNumericPickerModel> {
    use lumino_ui_egui::chrome::{SidebarNumericPickerKind, SidebarNumericPickerModel};
    let (track_id, kind, choices) = if let Some(id) = sidebar.port_picking_track {
        (
            id,
            SidebarNumericPickerKind::Port,
            crate::sidebar::Sidebar::PORT_CHOICES,
        )
    } else {
        let id = sidebar.channel_picking_track?;
        (
            id,
            SidebarNumericPickerKind::Channel,
            crate::sidebar::Sidebar::CHANNEL_CHOICES,
        )
    };
    let track = sidebar.tracks.iter().find(|track| track.id == track_id)?;
    let current = match kind {
        SidebarNumericPickerKind::Port => track.port,
        SidebarNumericPickerKind::Channel => track.channel,
    };
    Some(SidebarNumericPickerModel {
        track_id,
        kind,
        current,
        choices,
    })
}

/// 音轨行菜单模型；菜单未打开时返回 `None`。
pub(super) fn track_menu(sidebar: &crate::sidebar::Sidebar) -> Option<SidebarTrackMenuModel> {
    let track_id = sidebar.track_context_menu.target_track_id?;
    Some(SidebarTrackMenuModel {
        track_id,
        entries: TRACK_MENU_ITEMS.iter().map(|item| entry(*item)).collect(),
    })
}

/// 面板空白菜单模型；未打开时返回 `None`。
pub(super) fn panel_menu(sidebar: &crate::sidebar::Sidebar) -> Option<SidebarPanelMenuModel> {
    if !sidebar.panel_context_menu.is_open {
        return None;
    }
    let (x, y) = sidebar.panel_context_menu.mouse_pos.unwrap_or((0.0, 0.0));
    Some(SidebarPanelMenuModel {
        pos: egui::pos2(x, y),
        entries: PANEL_MENU_ITEMS
            .iter()
            .map(|item| panel_entry(*item))
            .collect(),
    })
}

/// 音轨菜单条目（图标 + 本地化文案，与 iced `item_icon` / `item_label` 同源）。
fn entry(item: TrackContextMenuItem) -> SidebarMenuEntry {
    let icon = match item {
        TrackContextMenuItem::Delete => Icon::ContextMenuDelete,
        TrackContextMenuItem::Rename => Icon::PencilOutline,
        TrackContextMenuItem::SetColor => Icon::ContextMenuColorPalette,
        TrackContextMenuItem::SetChannel => Icon::ContextMenuChannel,
        TrackContextMenuItem::SetPort => Icon::ContextMenuPort,
    };
    let label = match item {
        TrackContextMenuItem::Delete => tr!("删除"),
        TrackContextMenuItem::Rename => tr!("重命名"),
        TrackContextMenuItem::SetColor => tr!("设置颜色"),
        TrackContextMenuItem::SetChannel => tr!("设置通道"),
        TrackContextMenuItem::SetPort => tr!("设置端口"),
    };
    SidebarMenuEntry {
        icon,
        label: label.to_owned(),
    }
}

/// 面板菜单条目。
fn panel_entry(item: PanelContextMenuItem) -> SidebarMenuEntry {
    let (icon, label) = match item {
        PanelContextMenuItem::RecoverDeletedTrack => {
            (Icon::ContextMenuRecoverTrack, tr!("找回删除音轨"))
        }
    };
    SidebarMenuEntry {
        icon,
        label: label.to_owned(),
    }
}
