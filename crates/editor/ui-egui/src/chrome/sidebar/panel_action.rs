//! 左侧栏面板（音轨列表）的动作通道。
//!
//! 面板绘制在 `track_list.rs`；这里只承载「点到了什么」，由生产侧映射成
//! `Message::Sidebar(sidebar::Event::*)`。一帧只保留一个动作（与其它通道一致）。

/// 音轨列表上的动作。
#[derive(Debug, Clone, PartialEq)]
pub enum SidebarPanelAction {
    /// 选中音轨（携带生产音轨 ID，不是列表下标）。
    SelectTrack(usize),
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
    /// 确认重命名（回车）。
    RenameConfirmed(usize),
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
