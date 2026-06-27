//! Sidebar 事件子模块

use iced_core::Point;

use crate::Message;
use crate::sidebar::Route;

/// 侧边栏事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 路由更新
    RouteUpdated(Route),
    /// 面板切换
    PanelToggled(Route),
    /// 音轨选择
    TrackSelected(usize),
    /// 音轨静音切换
    TrackMuteToggled(usize),
    /// 添加音轨
    AddTrack,
    /// 添加音轨菜单切换
    AddTrackMenuToggled,
    /// 开始拖拽调整面板宽度
    ResizeDragStarted(Point),
    /// 拖拽中调整面板宽度
    ResizeDragged(Point),
    /// 结束拖拽调整面板宽度
    ResizeDragEnded,
    /// 自动化面板切换
    AutomationPanelToggled,
    /// 钢琴卷帘面板切换
    PianoRollToggled,
}

impl Event {
    pub const fn route_updated(r: Route) -> Message {
        Message::Sidebar(Self::RouteUpdated(r))
    }

    pub const fn panel_toggled(r: Route) -> Message {
        Message::Sidebar(Self::PanelToggled(r))
    }

    pub const fn track_selected(id: usize) -> Message {
        Message::Sidebar(Self::TrackSelected(id))
    }

    pub const fn track_mute_toggled(id: usize) -> Message {
        Message::Sidebar(Self::TrackMuteToggled(id))
    }

    pub const fn add_track() -> Message {
        Message::Sidebar(Self::AddTrack)
    }

    pub const fn add_track_menu_toggled() -> Message {
        Message::Sidebar(Self::AddTrackMenuToggled)
    }

    pub fn resize_drag_started() -> Message {
        Message::Sidebar(Self::ResizeDragStarted(Point::new(0.0, 0.0)))
    }

    pub fn resize_dragged() -> Message {
        Message::Sidebar(Self::ResizeDragged(Point::new(0.0, 0.0)))
    }

    pub const fn resize_drag_ended() -> Message {
        Message::Sidebar(Self::ResizeDragEnded)
    }

    pub const fn automation_panel_toggled() -> Message {
        Message::Sidebar(Self::AutomationPanelToggled)
    }

    pub const fn piano_roll_toggled() -> Message {
        Message::Sidebar(Self::PianoRollToggled)
    }
}
