//! 左侧栏面板（音轨列表）的动作通道。
//!
//! 面板绘制在 `track_list.rs`；这里只承载「点到了什么」（含行选择与独奏切换），
//! 由生产侧映射成 `Message::Sidebar(sidebar::Event::*)`。一帧只保留一个动作（与其它通道一致）。
//!
//! 行内重命名的失焦信号**不走这条单槽通道**：点击输入框外通常同时点中别的控件
//! （另一条音轨、添加行、工具栏…），挤进同一槽必然吞掉一个。它走独立的
//! [`SidebarRenameBlur`]，由宿主在分发任何面板动作之前先收口。

/// 音轨列表上的动作。
#[derive(Debug, Clone, PartialEq)]
pub enum SidebarPanelAction {
    /// 选中音轨（携带生产音轨 ID，不是列表下标）。
    SelectTrack(usize),
    /// 切换独奏（携带生产音轨 ID，不是列表下标）。
    TrackSoloToggled(usize),
    /// 添加音轨。
    AddTrack,
    /// 右键音轨行：请求打开该行的上下文菜单。
    OpenTrackMenu(usize),
    /// 右键面板空白：请求打开面板菜单（位置由宿主按光标写入）。
    OpenPanelMenu,
    /// 重命名草稿变化。
    RenameChanged {
        /// 音轨 ID。
        track_id: usize,
        /// 新草稿。
        value: String,
    },
    /// 选中预设颜色（`rgb` 为 sRGB 分量）。
    ColorPicked {
        /// 音轨 ID。
        track_id: usize,
        /// 颜色分量。
        rgb: (u8, u8, u8),
    },
    /// 恢复默认颜色。
    ColorReset(usize),
    /// 关闭取色器。
    ColorPickerClosed(usize),
    /// 选中端口（内部 0 基）。
    PortPicked {
        /// 音轨 ID。
        track_id: usize,
        /// 端口值。
        value: u8,
    },
    /// 关闭端口选择器。
    PortPickerClosed(usize),
    /// 选中通道（内部 0 基）。
    ChannelPicked {
        /// 音轨 ID。
        track_id: usize,
        /// 通道值。
        value: u8,
    },
    /// 关闭通道选择器。
    ChannelPickerClosed(usize),
    /// 按下面板右侧 6px 手柄（开始拖拽宽度）。
    ResizeStarted,
    /// 松开手柄。
    ResizeEnded,
    /// 音轨行按下并开始拖拽（`drag_started`）。
    ReorderStarted(usize),
    /// 拖拽中：插入位置（`0..=tracks.len()`）。
    ReorderMoved {
        /// 目标插入下标。
        index: usize,
    },
    /// 拖拽结束（生产用内部 hover_index 执行排序）。
    ReorderEnded {
        /// 最终插入下标（`0..=tracks.len()`）。
        index: usize,
    },
    /// 拖拽被取消（指针离开列表）。
    ReorderCancelled,
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

/// 重命名输入框失焦的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameBlur {
    /// 回车 / Tab / 点击输入框外的任意位置：提交当前草稿。
    Commit,
    /// Esc：放弃草稿，恢复原名。
    Cancel,
}

/// 一次重命名失焦信号：哪条音轨 + 最新草稿 + 结局。
///
/// 不复用 [`SidebarPanelAction`]：那条通道只有单槽，而「点击输入框外」通常恰好
/// 同时点中别的控件（点另一条音轨、点添加、点工具栏…），两者挤进同一槽必然
/// 吞掉一个。单独记一条信号，由宿主在分发任何面板动作**之前**先收口重命名。
///
/// 携带 `value` 而不是让宿主去读 [`SidebarPanelAction::RenameChanged`]：同帧的
/// 「最后一次击键 + 点击别处」会让 Changed 与失焦争抢单槽，草稿可能少一个字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarRenameBlur {
    /// 正在重命名的音轨 ID。
    pub track_id: usize,
    /// 失焦时的最新草稿。
    pub value: String,
    /// 结局。
    pub outcome: RenameBlur,
}

/// 失焦信号的临时数据键（每帧至多一条）。
const RENAME_BLUR_ID: &str = "lumino.sidebar-rename.blur";

/// 记录本帧重命名输入框失焦。
pub fn push_rename_blur(ctx: &egui::Context, blur: SidebarRenameBlur) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(RENAME_BLUR_ID), Some(blur)));
}

/// 取走本帧重命名失焦信号（宿主每帧调用一次，取走即清）。
pub fn take_rename_blur(ctx: &egui::Context) -> Option<SidebarRenameBlur> {
    ctx.data_mut(|d| d.remove_temp::<Option<SidebarRenameBlur>>(egui::Id::unique(RENAME_BLUR_ID)))
        .flatten()
}

/// 重命名会话「已抢过焦点」标记的 id。
///
/// 退出重命名时必须清除（见 `track_list::row` 的非重命名分支）：egui 的临时
/// 数据跨帧留存，不清的话下一次进入重命名不会 `request_focus`，输入框拿不到
/// 键盘——打字无反应，失焦提交也无从触发。
pub(super) fn rename_focus_once_id(track_id: usize) -> egui::Id {
    egui::Id::unique(("lumino.track-rename-focus", track_id))
}

/// 本次重命名会话「失焦已上报」标记的 id。
///
/// egui 0.37 的 `Response::lost_focus` 会连续两帧为真（为同帧焦点转移留出
/// 补偿窗口），不去重会把同一次失焦提交两遍。
pub(super) fn rename_blur_reported_id(track_id: usize) -> egui::Id {
    egui::Id::unique(("lumino.track-rename-blur-reported", track_id))
}

/// 清除重命名会话的临时标记（抢焦点 / 失焦已上报）。
///
/// 退出重命名后调用，保证下一个编辑会话从干净状态开始。
pub(super) fn reset_rename_edit_session(ctx: &egui::Context, track_id: usize) {
    ctx.data_mut(|d| {
        d.remove_temp::<bool>(rename_focus_once_id(track_id));
        d.remove_temp::<bool>(rename_blur_reported_id(track_id));
    });
}

/// 右键菜单动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMenuAction {
    /// 点了音轨行菜单的第 `index` 项。
    TrackItem {
        /// 目标音轨 ID。
        track_id: usize,
        /// 条目下标（顺序 = 生产 `TrackContextMenuItem`）。
        index: usize,
    },
    /// 点击菜单外 / Esc：关闭音轨行菜单。
    CloseTrackMenu,
    /// 点了面板空白菜单的第 `index` 项。
    PanelItem(usize),
    /// 关闭面板空白菜单。
    ClosePanelMenu,
}

/// 菜单动作通道 ID。
const MENU_ACTION_ID: &str = "lumino.sidebar-menu.action";

/// 记录本帧菜单动作。
pub fn push_menu_action(ctx: &egui::Context, action: SidebarMenuAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<SidebarMenuAction>>(egui::Id::unique(MENU_ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(MENU_ACTION_ID), Some(action)));
}

/// 取走本帧菜单动作。
pub fn take_menu_action(ctx: &egui::Context) -> Option<SidebarMenuAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<SidebarMenuAction>>(egui::Id::unique(MENU_ACTION_ID)))
        .flatten()
}
