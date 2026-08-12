//! Sidebar 事件子模块
//!
//! 包括侧边栏事件枚举及其依赖的分组和路由类型。

use iced_core::{Color, Point};
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::{PanelContextMenuItem, TrackContextMenuItem};

use crate::Message;

// ─── 分组 ID（从 sidebar/core.rs 迁入） ───

/// 侧边栏分组 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupId {
    /// 钢琴卷帘组（红色）
    PianoRoll,
    /// 工程走带组（绿色）
    Project,
    /// 播放器组（黄色）
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
                Language::ZhCn => "播放器",
                Language::EnUs => "Player",
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
        let translations = main_translations(lang);
        match self {
            Route::File => translations.sidebar_file,
            Route::Arrangement => translations.sidebar_arrangement,
            Route::Automation => translations.sidebar_automation,
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
    /// 音轨独奏切换
    TrackSoloToggled(usize),
    /// 多轨同时选择
    TracksSelected(Vec<usize>),
    /// 添加音轨
    AddTrack,
    /// 在指定音轨上方添加
    TrackAddAbove(usize),
    /// 在指定音轨下方添加
    TrackAddBelow(usize),
    /// 上移指定音轨
    TrackMoveUp(usize),
    /// 下移指定音轨
    TrackMoveDown(usize),
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
    /// 打开音轨列表面板空白区域右键菜单
    ///
    /// 注意：iced 0.14 的 `mouse_area::on_right_press` 仅传递 Message，
    /// 无法获取点击坐标。菜单固定显示在面板右上角（由 `panel_context_menu`
    /// 模块的 `positioned_menu` 决定）。
    PanelContextMenuOpened,
    /// 关闭音轨列表面板空白区域右键菜单
    PanelContextMenuClosed,
    /// 点击音轨列表面板空白区域右键菜单项
    PanelContextMenuItemClicked(PanelContextMenuItem),
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
    /// 重置音轨颜色为默认
    TrackColorReset(usize),
    /// 关闭颜色选择器
    TrackColorPickerClosed(usize),
    /// 音轨拖拽排序候选开始（左键按下，用于长按计时与移动跟踪）
    TrackReorderStarted(usize),
    /// 音轨拖拽排序中鼠标移动（列表局部坐标，用于更新插入指示位置）
    TrackReorderMoved { x: f32, y: f32 },
    /// 音轨拖拽排序结束（携带插入索引；`None` 表示未激活拖拽，不排序）
    TrackReorderEnded(Option<usize>),
    /// 取消音轨拖拽排序（不执行排序，仅清除候选状态）
    TrackReorderCancelled,
}

impl Event {
    pub const fn route_updated(route: Route) -> Message {
        Message::Sidebar(Self::RouteUpdated(route))
    }

    pub const fn panel_toggled(route: Route) -> Message {
        Message::Sidebar(Self::PanelToggled(route))
    }

    pub const fn track_selected(id: usize) -> Message {
        Message::Sidebar(Self::TrackSelected(id))
    }

    pub const fn track_mute_toggled(id: usize) -> Message {
        Message::Sidebar(Self::TrackMuteToggled(id))
    }

    pub const fn track_solo_toggled(id: usize) -> Message {
        Message::Sidebar(Self::TrackSoloToggled(id))
    }

    pub const fn tracks_selected(ids: Vec<usize>) -> Message {
        Message::Sidebar(Self::TracksSelected(ids))
    }

    pub const fn add_track() -> Message {
        Message::Sidebar(Self::AddTrack)
    }

    pub const fn track_add_above(id: usize) -> Message {
        Message::Sidebar(Self::TrackAddAbove(id))
    }

    pub const fn track_add_below(id: usize) -> Message {
        Message::Sidebar(Self::TrackAddBelow(id))
    }

    pub const fn track_move_up(id: usize) -> Message {
        Message::Sidebar(Self::TrackMoveUp(id))
    }

    pub const fn track_move_down(id: usize) -> Message {
        Message::Sidebar(Self::TrackMoveDown(id))
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

    pub const fn group_toggled(group: GroupId) -> Message {
        Message::Sidebar(Self::GroupToggled(group))
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

    pub const fn panel_context_menu_opened() -> Message {
        Message::Sidebar(Self::PanelContextMenuOpened)
    }

    pub const fn panel_context_menu_closed() -> Message {
        Message::Sidebar(Self::PanelContextMenuClosed)
    }

    pub const fn panel_context_menu_item_clicked(item: PanelContextMenuItem) -> Message {
        Message::Sidebar(Self::PanelContextMenuItemClicked(item))
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

    pub fn track_color_reset(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackColorReset(track_id))
    }

    pub fn track_color_picker_closed(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackColorPickerClosed(track_id))
    }

    pub const fn track_reorder_started(track_id: usize) -> Message {
        Message::Sidebar(Self::TrackReorderStarted(track_id))
    }

    pub const fn track_reorder_moved(x: f32, y: f32) -> Message {
        Message::Sidebar(Self::TrackReorderMoved { x, y })
    }

    pub const fn track_reorder_ended(insert_index: Option<usize>) -> Message {
        Message::Sidebar(Self::TrackReorderEnded(insert_index))
    }

    pub const fn track_reorder_cancelled() -> Message {
        Message::Sidebar(Self::TrackReorderCancelled)
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
