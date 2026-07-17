//! Sidebar 事件子模块
//!
//! 包括侧边栏事件枚举及其依赖的分组和路由类型。

use iced_core::{Color, Point};
use lumino_core::i18n::{Language, main_translations};
use lumino_message::TrackContextMenuItem;

use crate::Message;

// ─── 分组 ID（从 sidebar/core.rs 迁入） ───

/// 侧边栏分组 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupId {
    /// 钢琴卷帘组（红色）
    PianoRoll,
    /// 工程走带组（绿色）
    Project,
    /// 瀑布流播放器组（黄色）
    Waterfall,
    /// 渲染组（蓝色）
    Renderer,
}

impl GroupId {
    /// 父按钮灯条颜色（硬编码）
    pub fn parent_color(&self) -> Color {
        match self {
            GroupId::PianoRoll => Color::from_rgb(0.85, 0.15, 0.15),
            GroupId::Project => Color::from_rgb(0.15, 0.75, 0.35),
            GroupId::Waterfall => Color::from_rgb(0.85, 0.75, 0.10),
            GroupId::Renderer => Color::from_rgb(0.15, 0.45, 0.85),
        }
    }

    /// 子按钮灯条颜色（比父按钮浅）
    pub fn child_color(&self) -> Color {
        match self {
            GroupId::PianoRoll => Color::from_rgb(0.65, 0.35, 0.35),
            GroupId::Project => Color::from_rgb(0.35, 0.65, 0.45),
            GroupId::Waterfall => Color::from_rgb(0.65, 0.58, 0.30),
            GroupId::Renderer => Color::from_rgb(0.35, 0.55, 0.65),
        }
    }

    pub fn tooltip(&self, lang: Language) -> &'static str {
        match self {
            GroupId::PianoRoll => match lang {
                Language::ZhCn => "钢琴卷帘组",
                Language::EnUs => "Piano Roll",
            },
            GroupId::Project => match lang {
                Language::ZhCn => "工程走带",
                Language::EnUs => "Project",
            },
            GroupId::Waterfall => match lang {
                Language::ZhCn => "瀑布流播放器",
                Language::EnUs => "Waterfall Player",
            },
            GroupId::Renderer => match lang {
                Language::ZhCn => "渲染器",
                Language::EnUs => "Renderer",
            },
        }
    }
}

// ─── 路由（从 sidebar/core.rs 迁入） ───

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    File,
    Arrangement,
    Automation,
    VideoExport,
    AudioExport,
}

impl Route {
    pub fn tooltip(&self, lang: Language) -> &'static str {
        let t = main_translations(lang);
        match self {
            Route::File => t.sidebar_file,
            Route::Arrangement => t.sidebar_arrangement,
            Route::Automation => t.sidebar_automation,
            Route::VideoExport => match lang {
                Language::ZhCn => "视频渲染",
                Language::EnUs => "Video Render",
            },
            Route::AudioExport => match lang {
                Language::ZhCn => "音频渲染",
                Language::EnUs => "Audio Render",
            },
        }
    }
}

// ─── 事件（从 sidebar/event.rs 迁入） ───

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
    /// 分组切换
    GroupToggled(GroupId),
    /// 打开音轨选项卡右键菜单
    TrackContextMenuOpened(usize),
    /// 关闭音轨选项卡右键菜单
    TrackContextMenuClosed,
    /// 点击音轨选项卡右键菜单项
    TrackContextMenuItemClicked(usize, TrackContextMenuItem),
    /// 开始重命名音轨
    TrackRenameStarted(usize),
    /// 重命名输入变化
    TrackRenameChanged(usize, String),
    /// 确认重命名
    TrackRenameConfirmed(usize),
    /// 取消重命名
    TrackRenameCancelled(usize),
    /// 打开颜色选择器
    TrackColorPickerOpened(usize),
    /// 选择音轨颜色
    TrackColorSelected(usize, Color),
    /// 关闭颜色选择器
    TrackColorPickerClosed(usize),
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

    pub const fn group_toggled(g: GroupId) -> Message {
        Message::Sidebar(Self::GroupToggled(g))
    }

    pub const fn track_context_menu_opened(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackContextMenuOpened(track_id))
    }

    pub const fn track_context_menu_closed() -> Message {
        Message::Sidebar(Self::TrackContextMenuClosed)
    }

    pub const fn track_context_menu_item_clicked(
        track_id: usize,
        item: TrackContextMenuItem,
    ) -> Message {
        Message::Sidebar(Self::TrackContextMenuItemClicked(track_id, item))
    }

    pub fn track_rename_started(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackRenameStarted(track_id))
    }

    pub fn track_rename_changed(track_id: usize, value: String) -> Message {
        Message::Sidebar(Self::TrackRenameChanged(track_id, value))
    }

    pub fn track_rename_confirmed(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackRenameConfirmed(track_id))
    }

    pub fn track_rename_cancelled(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackRenameCancelled(track_id))
    }

    pub fn track_color_picker_opened(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackColorPickerOpened(track_id))
    }

    pub fn track_color_selected(track_id: usize, color: Color) -> Message {
        Message::Sidebar(Self::TrackColorSelected(track_id, color))
    }

    pub fn track_color_picker_closed(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackColorPickerClosed(track_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_context_menu_event_helpers() {
        let msg = Event::track_context_menu_opened(3);
        assert!(matches!(
            msg,
            Message::Sidebar(Event::TrackContextMenuOpened(3))
        ));

        let msg = Event::track_context_menu_closed();
        assert!(matches!(
            msg,
            Message::Sidebar(Event::TrackContextMenuClosed)
        ));

        let msg = Event::track_context_menu_item_clicked(2, TrackContextMenuItem::Delete);
        assert!(matches!(
            msg,
            Message::Sidebar(Event::TrackContextMenuItemClicked(
                2,
                TrackContextMenuItem::Delete
            ))
        ));
    }

    #[test]
    fn test_track_rename_event_helpers() {
        let msg = Event::track_rename_started(1);
        assert!(matches!(
            msg,
            Message::Sidebar(Event::TrackRenameStarted(1))
        ));

        let msg = Event::track_rename_changed(1, "New Name".to_string());
        assert!(matches!(
            msg,
            Message::Sidebar(Event::TrackRenameChanged(1, _))
        ));

        let msg = Event::track_rename_confirmed(1);
        assert!(matches!(
            msg,
            Message::Sidebar(Event::TrackRenameConfirmed(1))
        ));
    }

    #[test]
    fn test_track_color_event_helpers() {
        let color = Color::from_rgb(1.0, 0.0, 0.0);
        let msg = Event::track_color_selected(2, color);
        assert!(matches!(
            msg,
            Message::Sidebar(Event::TrackColorSelected(2, c)) if c == color
        ));
    }
}
