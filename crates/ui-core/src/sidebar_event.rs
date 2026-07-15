//! Sidebar 事件子模块
//!
//! 包括侧边栏事件枚举及其依赖的分组和路由类型。

use iced_core::{Color, Point};
use lumino_core::i18n::{Language, main_translations};

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
}
