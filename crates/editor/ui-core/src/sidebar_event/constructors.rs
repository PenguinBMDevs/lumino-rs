//! `Event` 的 `Message` 构造器 —— 统一包装为 `Message::Sidebar`
//!
//! 视图层只调用这些构造器，避免在 UI 代码中散落 `Message::Sidebar(...)` 包装。

use iced_core::{Color, Point};
use lumino_message::{PanelContextMenuItem, TrackContextMenuItem};

use super::{Event, GroupId, RollBarButton, Route};
use crate::Message;

impl Event {
    /// 构造"路由更新"的侧边栏消息
    pub const fn route_updated(route: Route) -> Message {
        Message::Sidebar(Self::RouteUpdated(route))
    }

    /// 构造"面板切换"的侧边栏消息
    pub const fn panel_toggled(route: Route) -> Message {
        Message::Sidebar(Self::PanelToggled(route))
    }

    /// 构造"音轨选择"的侧边栏消息
    pub const fn track_selected(id: usize) -> Message {
        Message::Sidebar(Self::TrackSelected(id))
    }

    /// 构造"音轨静音切换"的侧边栏消息
    pub const fn track_mute_toggled(id: usize) -> Message {
        Message::Sidebar(Self::TrackMuteToggled(id))
    }

    /// 构造"音轨独奏切换"的侧边栏消息
    pub const fn track_solo_toggled(id: usize) -> Message {
        Message::Sidebar(Self::TrackSoloToggled(id))
    }

    /// 构造"音轨增益变化"的侧边栏消息
    pub const fn track_gain_changed(id: usize, gain: f32) -> Message {
        Message::Sidebar(Self::TrackGainChanged(id, gain))
    }

    /// 构造"音轨声像变化"的侧边栏消息
    pub const fn track_pan_changed(id: usize, pan: f32) -> Message {
        Message::Sidebar(Self::TrackPanChanged(id, pan))
    }

    /// 构造"混音台浮动面板开关"的侧边栏消息
    pub const fn mixer_panel_toggled() -> Message {
        Message::Sidebar(Self::MixerPanelToggled)
    }

    /// 构造"混音台浮动面板最大化/最小化"的侧边栏消息
    pub const fn mixer_panel_maximize_toggled() -> Message {
        Message::Sidebar(Self::MixerPanelMaximizeToggled)
    }

    /// 构造"混音台浮动面板拖拽"的侧边栏消息
    pub const fn mixer_panel_dragged(dx: f32, dy: f32) -> Message {
        Message::Sidebar(Self::MixerPanelDragged(dx, dy))
    }

    /// 构造"多轨同时选择"的侧边栏消息
    pub const fn tracks_selected(ids: Vec<usize>) -> Message {
        Message::Sidebar(Self::TracksSelected(ids))
    }

    /// 构造"添加音轨"的侧边栏消息
    pub const fn add_track() -> Message {
        Message::Sidebar(Self::AddTrack)
    }

    /// 构造"在指定音轨上方添加"的侧边栏消息
    pub const fn track_add_above(id: usize) -> Message {
        Message::Sidebar(Self::TrackAddAbove(id))
    }

    /// 构造"在指定音轨下方添加"的侧边栏消息
    pub const fn track_add_below(id: usize) -> Message {
        Message::Sidebar(Self::TrackAddBelow(id))
    }

    /// 构造"上移指定音轨"的侧边栏消息
    pub const fn track_move_up(id: usize) -> Message {
        Message::Sidebar(Self::TrackMoveUp(id))
    }

    /// 构造"下移指定音轨"的侧边栏消息
    pub const fn track_move_down(id: usize) -> Message {
        Message::Sidebar(Self::TrackMoveDown(id))
    }

    /// 构造"开始拖拽调整面板宽度"的侧边栏消息
    pub fn resize_drag_started() -> Message {
        Message::Sidebar(Self::ResizeDragStarted(Point::new(0.0, 0.0)))
    }

    /// 构造"拖拽中调整面板宽度"的侧边栏消息
    pub fn resize_dragged() -> Message {
        Message::Sidebar(Self::ResizeDragged(Point::new(0.0, 0.0)))
    }

    /// 构造"结束拖拽调整面板宽度"的侧边栏消息
    pub const fn resize_drag_ended() -> Message {
        Message::Sidebar(Self::ResizeDragEnded)
    }

    /// 构造"自动化面板切换"的侧边栏消息
    pub const fn automation_panel_toggled() -> Message {
        Message::Sidebar(Self::AutomationPanelToggled)
    }

    /// 构造"钢琴卷帘面板切换"的侧边栏消息
    pub const fn piano_roll_toggled() -> Message {
        Message::Sidebar(Self::PianoRollToggled)
    }

    /// 构造"分组切换"的侧边栏消息
    pub const fn group_toggled(group: GroupId) -> Message {
        Message::Sidebar(Self::GroupToggled(group))
    }

    /// 构造"卷帘面板底部按钮切换"的侧边栏消息
    pub const fn roll_bar_toggled(button: RollBarButton) -> Message {
        Message::Sidebar(Self::RollBarToggled(button))
    }

    /// 构造"打开音轨选项卡右键菜单"的侧边栏消息
    pub const fn track_context_menu_opened(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackContextMenuOpened(track_id))
    }

    /// 构造"关闭音轨选项卡右键菜单"的侧边栏消息
    pub const fn track_context_menu_closed() -> Message {
        Message::Sidebar(Self::TrackContextMenuClosed)
    }

    /// 构造"点击音轨选项卡右键菜单项"的侧边栏消息
    pub const fn track_context_menu_item_clicked(
        track_id: usize,
        item: TrackContextMenuItem,
    ) -> Message {
        Message::Sidebar(Self::TrackContextMenuItemClicked(track_id, item))
    }

    /// 构造"打开侧边栏空白区域右键菜单"的侧边栏消息
    pub const fn panel_context_menu_opened() -> Message {
        Message::Sidebar(Self::PanelContextMenuOpened)
    }

    /// 构造"关闭侧边栏空白区域右键菜单"的侧边栏消息
    pub const fn panel_context_menu_closed() -> Message {
        Message::Sidebar(Self::PanelContextMenuClosed)
    }

    /// 构造"点击侧边栏空白区域右键菜单项"的侧边栏消息
    pub const fn panel_context_menu_item_clicked(item: PanelContextMenuItem) -> Message {
        Message::Sidebar(Self::PanelContextMenuItemClicked(item))
    }

    /// 构造"开始重命名音轨"的侧边栏消息
    pub fn track_rename_started(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackRenameStarted(track_id))
    }

    /// 构造"重命名输入变化"的侧边栏消息
    pub fn track_rename_changed(track_id: usize, value: String) -> Message {
        Message::Sidebar(Self::TrackRenameChanged(track_id, value))
    }

    /// 构造"确认重命名"的侧边栏消息
    pub fn track_rename_confirmed(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackRenameConfirmed(track_id))
    }

    /// 构造"取消重命名"的侧边栏消息
    pub fn track_rename_cancelled(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackRenameCancelled(track_id))
    }

    /// 构造"打开音轨颜色选择器"的侧边栏消息
    pub fn track_color_picker_opened(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackColorPickerOpened(track_id))
    }

    /// 构造"选择音轨颜色"的侧边栏消息
    pub fn track_color_selected(track_id: usize, color: Color) -> Message {
        Message::Sidebar(Self::TrackColorSelected(track_id, color))
    }

    /// 构造"重置音轨颜色为默认"的侧边栏消息
    pub fn track_color_reset(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackColorReset(track_id))
    }

    /// 构造"关闭音轨颜色选择器"的侧边栏消息
    pub fn track_color_picker_closed(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackColorPickerClosed(track_id))
    }

    /// 构造"音轨拖拽排序候选开始"的侧边栏消息
    pub const fn track_reorder_started(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackReorderStarted(track_id))
    }

    /// 构造"音轨拖拽排序中鼠标移动"的侧边栏消息
    pub const fn track_reorder_moved(x: f32, y: f32) -> Message {
        Message::Sidebar(Self::TrackReorderMoved { x, y })
    }

    /// 构造"音轨拖拽排序结束"的侧边栏消息
    pub const fn track_reorder_ended(insert_index: Option<usize>) -> Message {
        Message::Sidebar(Self::TrackReorderEnded(insert_index))
    }

    /// 构造"取消音轨拖拽排序"的侧边栏消息
    pub const fn track_reorder_cancelled() -> Message {
        Message::Sidebar(Self::TrackReorderCancelled)
    }
}
