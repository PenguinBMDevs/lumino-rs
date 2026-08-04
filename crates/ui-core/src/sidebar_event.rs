//! Sidebar 事件子模块
//!
//! 包括侧边栏事件枚举及其依赖的分组和路由类型。

use iced_core::{Color, Point};
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::{PanelContextMenuItem, TrackContextMenuItem};
use lumino_note_core::event::{AutomationTarget, ScaleType, SegmentShape};
use std::collections::HashSet;

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
    EventList,
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
            Route::EventList => translations.sidebar_event_list,
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

// ─── 事件浏览器共享类型（从 lumino-ui 上移，避免循环依赖） ───

/// 事件列表右键菜单项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventListMenuItem {
    /// 在上方插入
    InsertAbove,
    /// 在下方插入
    InsertBelow,
    /// 删除
    Delete,
}

/// 事件浏览器表格行点击时产生的跳转请求。
#[derive(Clone, Debug, PartialEq)]
pub struct JumpRequest {
    pub tick: u32,
    /// 音符事件：`Some((track, key))`；其他事件：`None`。
    pub note: Option<(u16, u8)>,
}

/// 音符引用：足够定位一个音符的所有字段。
///
/// `id` 用于 `Arc::make_mut` 后的 retain，`start_tick` / `key` / `track` 用于
/// 寻址。`end_tick` / `velocity` 是当前值，便于编辑器实时显示。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoteRef {
    pub id: u64,
    pub start_tick: u32,
    pub end_tick: u32,
    pub key: u8,
    pub velocity: u8,
    pub track: u16,
}

/// 文本类事件种类：Marker / Lyrics / Chord，按归属区分 conductor 级 vs per-track。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextEventKind {
    /// Marker：conductor 级
    Marker,
    /// Conductor 级歌词（track 0 的 FF 05）
    ConductorLyrics,
    /// Conductor 级和弦（仅 .yin 格式）
    ConductorChord,
    /// Per-track 歌词
    Lyrics { track: u16 },
    /// Per-track 和弦
    Chord { track: u16 },
}

/// 右键编辑请求：cell 上右键时写入状态，由上层 UI 取出分派。
#[derive(Clone, Debug, PartialEq)]
pub enum EditRequest {
    /// Automation 的 tick 编辑（位置移动）。`value`/`shape` 为当前值，
    /// 应用时保留（只改 tick）。
    AutoTick {
        tick: u32,
        value: f32,
        shape: SegmentShape,
    },
    /// Automation 的 value 编辑。`shape` 为当前值，应用时保留（只改 value）。
    AutoValue {
        tick: u32,
        value: f32,
        shape: SegmentShape,
    },
    /// Automation 的 shape 编辑。`value` 为当前值，应用时保留（只改 shape）。
    AutoShape {
        tick: u32,
        value: f32,
        shape: SegmentShape,
    },
    /// 音符 start_tick 编辑（保持 gate 不变，end_tick 跟随平移）
    NoteStartTick { note: NoteRef },
    /// 音符 end_tick 编辑（gate 随之变化）
    NoteEndTick { note: NoteRef },
    /// 音符 gate（长度）编辑（实际改 end_tick = start_tick + gate）
    NoteGate { note: NoteRef },
    /// 音符 key 编辑
    NoteKey { note: NoteRef },
    /// 音符 velocity 编辑
    NoteVelocity { note: NoteRef },
    /// TimeSig 的 tick 编辑（按 tick 寻址，避免 sort 后 idx 失效）
    TimeSigTick { tick: u32 },
    /// TimeSig 的 numerator 编辑
    TimeSigNumerator { tick: u32 },
    /// TimeSig 的 denominator 编辑（2 的幂次：2 = 4, 3 = 8）
    TimeSigDenominator { tick: u32 },
    /// KeySig 的 tick 编辑
    KeySigTick { tick: u32 },
    /// KeySig 的 root 编辑（根音 pitch class 0-11）
    KeySigRoot { tick: u32 },
    /// KeySig 的 scale 编辑（音阶类型）
    KeySigScale { tick: u32 },
    /// Program Change 的 tick 编辑
    PcTick { tick: u32 },
    /// Program Change 的 program 编辑
    PcProgram { tick: u32 },
    /// 文本类事件（Marker/Lyrics/Chord）的 tick 编辑
    TextEventTick { kind: TextEventKind, tick: u32 },
    /// 文本类事件的 text 编辑
    TextEventText { kind: TextEventKind, tick: u32 },
    /// 删除当前选中的事件（多选批量删除）
    DeleteSelected,
    /// 在指定 tick 上方插入新事件（复制该行 tick 的默认值）
    InsertAbove { tick: u32 },
    /// 在指定 tick 下方插入新事件（复制该行 tick 的默认值）
    InsertBelow { tick: u32 },
    /// 新建第一个事件（空表格加号触发）
    InsertFirst,
}

impl EditRequest {
    /// 是否为位置（小节/小节内 tick）编辑。
    pub fn is_position_edit(&self) -> bool {
        matches!(
            self,
            EditRequest::NoteStartTick { .. }
                | EditRequest::NoteEndTick { .. }
                | EditRequest::TimeSigTick { .. }
                | EditRequest::KeySigTick { .. }
                | EditRequest::PcTick { .. }
                | EditRequest::TextEventTick { .. }
        )
    }

    /// 是否为纯数值编辑。
    pub fn is_number_edit(&self) -> bool {
        matches!(
            self,
            EditRequest::AutoValue { .. }
                | EditRequest::NoteGate { .. }
                | EditRequest::NoteKey { .. }
                | EditRequest::NoteVelocity { .. }
                | EditRequest::TimeSigNumerator { .. }
                | EditRequest::TimeSigDenominator { .. }
                | EditRequest::KeySigRoot { .. }
                | EditRequest::PcProgram { .. }
        )
    }

    /// 是否为文本编辑。
    pub fn is_text_edit(&self) -> bool {
        matches!(self, EditRequest::TextEventText { .. })
    }

    /// 是否为下拉选择编辑。
    pub fn is_choice_edit(&self) -> bool {
        matches!(
            self,
            EditRequest::AutoShape { .. } | EditRequest::KeySigScale { .. }
        )
    }
}

// ─── 事件浏览器状态与导航类型 ───

/// 事件浏览器中选中的条目。
///
/// `Automation` 统一覆盖 CC / PitchBend / RPN / NRPN / Tempo。
/// `track` 对 Tempo 无意义（用 0），其他类型为所属音轨索引。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum SelectedItem {
    ProjectJson,
    MappingJson,
    TimeSig,
    /// 调号事件（全局，conductor 级）
    KeySig,
    /// 标记事件（全局，conductor 级）
    Markers,
    /// 歌词事件（全局，conductor 级，track 0 的 FF 05）
    ConductorLyrics,
    /// 和弦事件（全局，conductor 级）
    ConductorChord,
    Notes {
        track: u16,
    },
    ProgramChange {
        track: u16,
    },
    Automation {
        track: u16,
        target: AutomationTarget,
    },
    /// 歌词事件（per-track）
    Lyrics {
        track: u16,
    },
    /// 和弦事件（per-track）
    Chord {
        track: u16,
    },
}

/// 左侧树节点展开状态键。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ArchiveKey {
    Conductor,
    Port(u8),
    Channel(u8, u8),
    Track(u16),
}

/// 事件浏览器状态。
///
/// 所有字段均为 UI 框架无关类型，便于在 egui / iced Canvas 之间复用。
#[derive(Clone, Debug, PartialEq)]
pub struct EventBrowserState {
    /// 左侧树中已展开的节点键集合。
    pub expanded_keys: HashSet<ArchiveKey>,
    /// 当前选中的左侧树叶子项。
    pub selected_item: Option<SelectedItem>,
    /// 当前选中的音轨索引（树中点击 track 行时设置）。
    pub selected_track: Option<u16>,
    /// 事件列表当前页码（0-based）。切换 `selected_item` 时重置为 0。
    pub event_page: usize,
    /// 表格行多选：选中事件的 tick 集合。切换 `selected_item` 时自动清空。
    pub selected_ticks: HashSet<u32>,
    /// 上次单击的 tick（用于 Shift 范围选择锚点）。
    pub last_clicked_tick: Option<u32>,
    pub(super) fingerprint: Option<u64>,
    pub(super) split_ratio: f32,
}

impl Default for EventBrowserState {
    fn default() -> Self {
        Self {
            expanded_keys: HashSet::new(),
            selected_item: None,
            selected_track: None,
            event_page: 0,
            selected_ticks: HashSet::new(),
            last_clicked_tick: None,
            fingerprint: None,
            split_ratio: 0.45,
        }
    }
}

/// 事件列表待执行的 editor 数据修改操作。
///
/// 由 `Sidebar` 根据 `EditRequest` / 上下文菜单 / popup 确认生成，
/// 供 `Root` 在 `sidebar.update()` 之后取出并应用到 `EditorData`。
#[derive(Clone, Debug, PartialEq)]
pub enum EventListAction {
    /// 删除当前选中事件。
    DeleteSelected,
    /// 在指定 tick 上方插入新事件。
    InsertAbove(u32),
    /// 在指定 tick 下方插入新事件。
    InsertBelow(u32),
    /// 新建第一个事件。
    InsertFirst,
    /// 设置拍号事件。
    SetTimeSig {
        tick: u32,
        numerator: u8,
        denominator: u8,
    },
    /// 设置调号事件。
    SetKeySig {
        tick: u32,
        root: u8,
        scale: ScaleType,
    },
    /// 设置标记事件。
    SetMarker { tick: u32, text: String },
    /// 设置歌词事件。
    SetLyrics { track: u16, tick: u32, text: String },
    /// 设置和弦事件。
    SetChord { track: u16, tick: u32, text: String },
    /// 设置音色变换事件。
    SetProgramChange { track: u16, tick: u32, program: u8 },
    /// 设置自动化事件。
    SetAutomation {
        track: u16,
        target: AutomationTarget,
        tick: u32,
        value: f32,
        shape: SegmentShape,
    },
    /// 设置音符起始 tick。
    SetNoteStart { note: NoteRef, new_tick: u32 },
    /// 设置音符结束 tick。
    SetNoteEnd { note: NoteRef, new_end_tick: u32 },
    /// 设置音符 gate（长度）。
    SetNoteGate { note: NoteRef, gate: f32 },
    /// 设置音符音高。
    SetNoteKey { note: NoteRef, new_key: u8 },
    /// 设置音符力度。
    SetNoteVelocity { note: NoteRef, new_velocity: u8 },
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
    /// 事件列表垂直滚动偏移与视口高度更新
    EventListScrolled(f32, f32),
    /// 事件列表行点击（单选/跳转锚点）
    EventListRowClicked(u32),
    /// 事件列表右键行头打开上下文菜单
    EventListContextMenuOpened(u32),
    /// 事件列表关闭上下文菜单
    EventListContextMenuClosed,
    /// 事件列表上下文菜单项点击
    EventListContextMenuItemClicked(EventListMenuItem),
    /// 事件列表跳转请求
    EventListJump(JumpRequest),
    /// 事件列表编辑/操作请求
    EventListEdit(EditRequest),
    /// 事件列表 popup 编辑器确认（pending 原始字符串）
    EventListPopupConfirm(EditRequest, String),
    /// 事件列表 popup 编辑器取消
    EventListPopupCancel,
    /// 事件浏览器：切换树节点展开状态
    EventListTreeToggled(ArchiveKey),
    /// 事件浏览器：选中树叶子项
    EventListItemSelected(SelectedItem),
    /// 事件浏览器：翻页
    EventListPageChanged(usize),
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

    pub fn event_list_scrolled(offset: f32, viewport_height: f32) -> Message {
        Message::Sidebar(Self::EventListScrolled(offset, viewport_height))
    }

    pub const fn event_list_row_clicked(tick: u32) -> Message {
        Message::Sidebar(Self::EventListRowClicked(tick))
    }

    pub const fn event_list_context_menu_opened(tick: u32) -> Message {
        Message::Sidebar(Self::EventListContextMenuOpened(tick))
    }

    pub const fn event_list_context_menu_closed() -> Message {
        Message::Sidebar(Self::EventListContextMenuClosed)
    }

    pub const fn event_list_context_menu_item_clicked(item: EventListMenuItem) -> Message {
        Message::Sidebar(Self::EventListContextMenuItemClicked(item))
    }

    pub const fn event_list_jump(req: JumpRequest) -> Message {
        Message::Sidebar(Self::EventListJump(req))
    }

    pub const fn event_list_edit(req: EditRequest) -> Message {
        Message::Sidebar(Self::EventListEdit(req))
    }

    pub const fn event_list_popup_confirm(req: EditRequest, value: String) -> Message {
        Message::Sidebar(Self::EventListPopupConfirm(req, value))
    }

    pub const fn event_list_popup_cancel() -> Message {
        Message::Sidebar(Self::EventListPopupCancel)
    }

    pub const fn event_list_tree_toggled(key: ArchiveKey) -> Message {
        Message::Sidebar(Self::EventListTreeToggled(key))
    }

    pub const fn event_list_item_selected(item: SelectedItem) -> Message {
        Message::Sidebar(Self::EventListItemSelected(item))
    }

    pub const fn event_list_page_changed(page: usize) -> Message {
        Message::Sidebar(Self::EventListPageChanged(page))
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

    #[test]
    fn test_event_list_event_helpers() {
        let msg = Event::event_list_row_clicked(120);
        assert!(matches!(
            msg,
            Message::Sidebar(Event::EventListRowClicked(120))
        ));

        let msg = Event::event_list_context_menu_opened(120);
        assert!(matches!(
            msg,
            Message::Sidebar(Event::EventListContextMenuOpened(120))
        ));

        let msg = Event::event_list_context_menu_closed();
        assert!(matches!(
            msg,
            Message::Sidebar(Event::EventListContextMenuClosed)
        ));

        let msg = Event::event_list_context_menu_item_clicked(EventListMenuItem::Delete);
        assert!(matches!(
            msg,
            Message::Sidebar(Event::EventListContextMenuItemClicked(
                EventListMenuItem::Delete
            ))
        ));

        let req = JumpRequest {
            tick: 100,
            note: None,
        };
        let msg = Event::event_list_jump(req.clone());
        assert!(matches!(msg, Message::Sidebar(Event::EventListJump(r)) if r == req));

        let req = EditRequest::DeleteSelected;
        let msg = Event::event_list_edit(req.clone());
        assert!(matches!(
            msg,
            Message::Sidebar(Event::EventListEdit(EditRequest::DeleteSelected))
        ));

        let msg = Event::event_list_popup_confirm(EditRequest::InsertFirst, "42".to_string());
        assert!(matches!(
            msg,
            Message::Sidebar(Event::EventListPopupConfirm(EditRequest::InsertFirst, _))
        ));

        let msg = Event::event_list_popup_cancel();
        assert!(matches!(msg, Message::Sidebar(Event::EventListPopupCancel)));
    }
}
