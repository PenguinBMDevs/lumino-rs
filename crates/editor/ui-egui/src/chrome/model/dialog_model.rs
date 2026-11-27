//! 独立对话框整窗模型枚举：按 `DialogType` 分派给各渲染模块。
//!
//! 从 `dialog.rs` 拆出：模型结构体按领域继续留在 `dialog.rs`，本文件只保留
//! 「一个窗口一帧用哪个模型」的枚举，避免单文件随对话框数量增长而超限。

use super::{
    AboutModel, AutosaveRecoverModel, BatchEditModel, BrushSettingsModel, CloudBrowserModel,
    CloudConnectModel, CloudNoticeModel, CollaborationModel, CustomPrecisionModel,
    DeviceWarningModel, DrawSettingsModel, ExportProgressModel, HelpModel, HistoryModel,
    LoadConfirmModel, MaterialDeleteModel, MemoryMonitorModel, PortSoundfontsModel,
    PortSoundfontsUnsupportedModel, ProgressModel, ProjectSettingsModel, RecoverTrackModel,
    SaveConfirmModel, SettingsSnapshot, SpeedChangeModel, TempoMaxBpmModel,
    VideoExportOverlayModel,
};

/// 独立对话框窗口的只读模型。
#[derive(Debug, Clone)]
pub enum DialogModel {
    /// 未保存更改确认（`DialogType::SaveConfirm`）。
    SaveConfirm(SaveConfirmModel),
    /// 设置对话框「常规」页（`DialogType::Settings`，仅 `selected == 0`）。
    ///
    /// 装箱：`SettingsSnapshot` 含全部文案与调色板，比确认对话框大一个数量级。
    Settings(Box<SettingsSnapshot>),
    /// 工程设置（`DialogType::ProjectSettings`，系统原生标题栏，整窗内容）。
    ProjectSettings(Box<ProjectSettingsModel>),
    /// 端口音色库（`DialogType::PortSoundfonts`，REND-005）。
    PortSoundfonts(Box<PortSoundfontsModel>),
    /// 「端口音色库未生效」提示（`DialogType::PortSoundfontsUnsupported`，REND-005）。
    PortSoundfontsUnsupported(Box<PortSoundfontsUnsupportedModel>),
    /// 撤销历史（`DialogType::History`，只读时间线 + 跳转）。
    History(Box<HistoryModel>),
    /// 批量编辑（`DialogType::BatchEdit`）。
    BatchEdit(Box<BatchEditModel>),
    /// 变速（`DialogType::SpeedChange`）。
    SpeedChange(Box<SpeedChangeModel>),
    /// 素材删除确认（`DialogType::MaterialDelete`，DRAW-045）。
    MaterialDelete(Box<MaterialDeleteModel>),
    /// 自定义 Tempo BPM 上限（`DialogType::TempoMaxBpm`，DRAW-045）。
    TempoMaxBpm(Box<TempoMaxBpmModel>),
    /// 自定义精度（`DialogType::CustomPrecision`）。
    CustomPrecision(Box<CustomPrecisionModel>),
    /// 音频导出进度（`DialogType::ExportProgress`，只读）。
    ExportProgress(Box<ExportProgressModel>),
    /// Help 窗口（`DialogType::Help`）。
    Help(Box<HelpModel>),
    /// 关于窗口（`DialogType::About`）：品牌行 + 版本号。
    About(Box<AboutModel>),
    /// 云存储连接面板（`DialogType::CloudConnect`）。
    CloudConnect(Box<CloudConnectModel>),
    /// 内存监控（`DialogType::MemoryMonitor`）。
    MemoryMonitor(Box<MemoryMonitorModel>),
    /// 音符画设置（`DialogType::DrawSettings`）。
    DrawSettings(Box<DrawSettingsModel>),
    /// 加载确认（`DialogType::LoadConfirm`）。
    LoadConfirm(Box<LoadConfirmModel>),
    /// 画刷设置（`DialogType::BrushSettings`）。
    BrushSettings(Box<BrushSettingsModel>),
    /// 找回删除音轨（`DialogType::RecoverTrack`）。
    RecoverTrack(Box<RecoverTrackModel>),
    /// 多人协作（`DialogType::Collaboration`）。
    Collaboration(Box<CollaborationModel>),
    /// 云存储连接面板（`DialogType::CloudConnect`）。
    CloudBrowser(Box<CloudBrowserModel>),
    /// 云存储断连提醒（`DialogType::CloudNotice`）。
    CloudNotice(Box<CloudNoticeModel>),
    /// 视频导出进度覆盖层（`DialogType::VideoExport` 的导出中/收尾/完成/失败）。
    VideoExportOverlay(Box<VideoExportOverlayModel>),
    /// 自动保存恢复（`DialogType::AutosaveRecover`，DEBT-12）。
    AutosaveRecover(Box<AutosaveRecoverModel>),
    /// 混音台（`DialogType::Mixer`，独立 OS 窗口）。
    Mixer(Box<crate::mixer::MixerDialogModel>),
    /// 启动/加载进度窗（独立窗口，无取消按钮）。
    Progress(ProgressModel),
    /// 设备检查警告窗（独立窗口，勾选框 + 两个按钮）。
    DeviceWarning(DeviceWarningModel),
}

impl DialogModel {
    /// 对话框是否自己绘制 30px 标题栏（此时不能再叠通用 popup 标题栏）。
    ///
    /// 只有保存确认与设置页自己画了红绿灯；其余对话框在 `titlebar == true` 时
    /// 主动让出顶部 30px，交给 [`crate::chrome::PopupTitlebarModel`]。
    pub fn draws_own_titlebar(&self) -> bool {
        matches!(self, Self::SaveConfirm(_) | Self::Settings(_))
    }
}
