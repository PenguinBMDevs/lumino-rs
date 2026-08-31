//! 平台适配 — yinhe `platform/mod.rs:129` + `platform/macos.rs:1064` 的 iced 迁移桩
//!
//! - `MenuAction` 与 yinhe 保持同源（文件/编辑/播放/最近文件/App 菜单）
//! - macOS 侧以 `muda` 构建原生 `NSMenu`，加速键与 `Keybindings` 同步，
//!   语言切换时刷新文本；非 macOS 侧为 `stub` 空操作
//! - 复用 `lumino` 的窗口管理（`DialogManager` 独立窗口 + `winit`），
//!   字体/配色走 `Theme`，图标走 SVG

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(target_os = "macos"))]
mod stub;

/// 原生菜单动作（与 yinhe `platform::MenuAction` 同步）
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    NewProject,
    Open,
    Save,
    SaveAs,
    CloseDocument,
    ExportAudio,
    ExportMidi,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Duplicate,
    Delete,
    TransposeUp,
    TransposeDown,
    DedupWithinTrack,
    DedupAcrossTracks,
    TogglePlay,
    Stop,
    ToggleRecord,
    ToggleStepInput,
    SetFollowMode(u8),
    OpenRecent(String),
    Settings,
    ProjectSettings,
    Exit,
    About,
    Hide,
    HideOthers,
    ShowAll,
}

/// 原生菜单栏句柄（生命周期与应用一致，菜单项由 `muda` 持有保活）
pub struct MenuBar {
    inner: MenuBarInner,
}

impl MenuBar {
    pub fn new() -> Self {
        Self {
            inner: MenuBarInner::new(),
        }
    }

    /// 轮询 `muda` 事件队列，返回待处理的 `MenuAction`
    pub fn poll(&mut self) -> Vec<MenuAction> {
        self.inner.poll()
    }

    /// 轮询 Finder/桌面 `Open With` 传入的文件路径（仅 macOS）
    pub fn poll_open_files(&mut self) -> Vec<String> {
        self.inner.poll_open_files()
    }
}

impl Default for MenuBar {
    fn default() -> Self {
        Self::new()
    }
}

/// 文档脏点（红绿灯圆点）
pub fn set_document_edited(edited: bool) {
    set_document_edited_inner(edited);
}

/// Dock 跳动
pub fn request_user_attention() {
    request_user_attention_inner();
}

/// App Nap 控制（播放时阻止系统降频）
pub fn set_app_nap_enabled(enabled: bool) {
    set_app_nap_enabled_inner(enabled);
}

/// 禁用系统标题栏背景拖动（`mouseDownCanMoveWindow` → NO）
pub fn disable_background_window_drag() {
    disable_background_window_drag_inner();
}

#[cfg(target_os = "macos")]
use macos::{
    MenuBarInner, disable_background_window_drag as disable_background_window_drag_inner,
    request_user_attention as request_user_attention_inner,
    set_app_nap_enabled as set_app_nap_enabled_inner,
    set_document_edited as set_document_edited_inner,
};
#[cfg(not(target_os = "macos"))]
use stub::{
    MenuBarInner, disable_background_window_drag as disable_background_window_drag_inner,
    request_user_attention as request_user_attention_inner,
    set_app_nap_enabled as set_app_nap_enabled_inner,
    set_document_edited as set_document_edited_inner,
};
