//! 对话框 — yinhe `dialogs/*` 的 iced 迁移桩
//!
//! 原 `yinhe-egui/src/dialogs` 下 23 文件（`archive_picker 631`、`export 418`、
//! `settings/* 7`、`new_track 406` 等）在 lumino 侧改以 iced 实现：
//! - 每个对话框以 `container + column + button` 组合搭建，样式走 `lumino_ui_core::Theme`
//!   与 `lumino_ui_core::resources::icon` 的 SVG 直渲（`define_icons!`），字体走
//!   `Theme`（不引入 `egui`）
//! - 独立窗口复用 `lumino_dialog::DialogManager` 的独立 winit 窗口模式
//!  （每个对话框为独立 `DialogWindow`，生命周期由 `DialogManager` 统一管理）；
//!   yinhe 侧以 `YinheDialogType` 枚举对齐 `lumino_dialog::DialogType` 的独立窗口语义
//! - 具体业务（压缩包条目过滤、导出进度、通道分配等）由 Host/Runner 注入，
//!   本层仅保留 UI 状态与视图骨架

pub mod archive_picker;
pub mod audio_device_switch;
pub mod export;
pub mod gpu_device_lost;
pub mod load_error;
pub mod loading_overlay;
pub mod memory_breakdown;
pub mod new_track;
pub mod ppq_rescale_confirm;
pub mod prop_panels;
pub mod rescale_overlay;
pub mod save_overlay;
pub mod settings;
pub mod system_monitor;
pub mod unsaved;

/// Yinhe 侧对话框类型（对齐 `lumino_dialog::DialogType` 的独立窗口语义）
///
/// 每个变体对应一个 `DialogWindow`；`DialogManager` 按此类型批量创建/销毁窗口。
/// 复用 `lumino_dialog::DialogManager` 的分帧初始化与主题同步逻辑：
/// - 阶段 1：创建 winit 窗口（hidden）
/// - 阶段 2：创建 wgpu 图形上下文
/// - 阶段 3：创建 iced UI Host、同步状态并显示窗口
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum YinheDialogType {
    ArchivePicker,
    PasswordPrompt,
    ExportProgress,
    ExportCompleted,
    ExportSettings,
    NewTrack,
    LoadError,
    MemoryBreakdown,
    AudioDeviceSwitch,
    GpuDeviceLost,
    LoadingOverlay,
    PpqRescaleConfirm,
    TrackProps,
    ProjectProps,
    RescaleOverlay,
    SaveOverlay,
    SystemMonitor,
    Unsaved,
    Settings,
}

impl YinheDialogType {
    /// 窗口标题（与 yinhe `t!("dialog.xxx.title")` 对齐）
    pub fn title(self) -> &'static str {
        match self {
            Self::ArchivePicker => "archive_picker",
            Self::PasswordPrompt => "password_prompt",
            Self::ExportProgress => "export_progress",
            Self::ExportCompleted => "export_completed",
            Self::ExportSettings => "export_settings",
            Self::NewTrack => "new_track",
            Self::LoadError => "load_error",
            Self::MemoryBreakdown => "memory_breakdown",
            Self::AudioDeviceSwitch => "audio_device_switch",
            Self::GpuDeviceLost => "gpu_device_lost",
            Self::LoadingOverlay => "loading",
            Self::PpqRescaleConfirm => "ppq_rescale_confirm",
            Self::TrackProps => "track_props",
            Self::ProjectProps => "project_props",
            Self::RescaleOverlay => "rescale_overlay",
            Self::SaveOverlay => "save_overlay",
            Self::SystemMonitor => "system_monitor",
            Self::Unsaved => "unsaved",
            Self::Settings => "settings",
        }
    }

    /// 默认窗口尺寸（与 yinhe `viewport_builder(..., [w, h], ...)` 对齐）
    pub fn default_size(self) -> [f32; 2] {
        match self {
            Self::ArchivePicker => [560.0, 400.0],
            Self::PasswordPrompt => [460.0, 160.0],
            Self::ExportProgress => [320.0, 310.0],
            Self::ExportCompleted => [320.0, 160.0],
            Self::ExportSettings => [320.0, 220.0],
            Self::NewTrack => [400.0, 320.0],
            Self::LoadError => [420.0, 120.0],
            Self::MemoryBreakdown => [360.0, 400.0],
            Self::AudioDeviceSwitch => [460.0, 440.0],
            Self::GpuDeviceLost => [460.0, 150.0],
            Self::LoadingOverlay => [380.0, 160.0],
            Self::PpqRescaleConfirm => [380.0, 170.0],
            Self::TrackProps => [380.0, 520.0],
            Self::ProjectProps => [420.0, 520.0],
            Self::RescaleOverlay => [320.0, 120.0],
            Self::SaveOverlay => [320.0, 120.0],
            Self::SystemMonitor => [360.0, 240.0],
            Self::Unsaved => [340.0, 130.0],
            Self::Settings => [760.0, 620.0],
        }
    }
}
