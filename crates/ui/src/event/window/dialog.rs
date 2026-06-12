//! 对话框相关事件

#[derive(Debug, Clone)]
pub enum Event {
    /// 打开自定义精度对话框窗口
    OpenCustomPrecisionDialog,
    /// 打开加载确认对话框
    OpenLoadConfirmDialog { path: String, size_mb: f64 },
    /// 关闭自定义精度对话框窗口
    CloseCustomPrecisionDialog,
    /// 应用自定义精度设置 (numerator, denominator)
    ApplyCustomPrecision(u32, u32),
    /// 打开协作对话框窗口
    OpenCollaborationDialog,
    /// 关闭协作对话框窗口
    CloseCollaborationDialog,
    /// 打开音符变速对话框
    OpenSpeedChangeDialog,
    /// 关闭音符变速对话框
    CloseSpeedChangeDialog,
    /// 确认音符变速
    ConfirmSpeedChange(f32),
    /// 打开工程设置对话框
    OpenProjectSettingsDialog,
    /// 关闭工程设置对话框
    CloseProjectSettingsDialog,
    /// 应用工程设置
    ApplyProjectSettings {
        title: String,
        tempo: f64,
        copyright: String,
    },
}

impl Event {
    pub fn display_name(&self) -> String {
        match self {
            Self::OpenCustomPrecisionDialog => "自定义精度".to_string(),
            Self::OpenLoadConfirmDialog { .. } => "加载确认".to_string(),
            Self::CloseCustomPrecisionDialog => "关闭自定义精度".to_string(),
            Self::ApplyCustomPrecision(_, _) => "应用精度设置".to_string(),
            Self::OpenCollaborationDialog => "协作".to_string(),
            Self::CloseCollaborationDialog => "关闭协作".to_string(),
            Self::OpenSpeedChangeDialog => "音符变速".to_string(),
            Self::CloseSpeedChangeDialog => "关闭音符变速".to_string(),
            Self::ConfirmSpeedChange(_) => "确认变速".to_string(),
            Self::OpenProjectSettingsDialog => "工程设置".to_string(),
            Self::CloseProjectSettingsDialog => "关闭工程设置".to_string(),
            Self::ApplyProjectSettings { .. } => "应用工程设置".to_string(),
        }
    }

    pub const fn open_custom_precision_dialog() -> Self {
        Self::OpenCustomPrecisionDialog
    }
    pub const fn close_custom_precision_dialog() -> Self {
        Self::CloseCustomPrecisionDialog
    }
    pub const fn apply_custom_precision(numerator: u32, denominator: u32) -> Self {
        Self::ApplyCustomPrecision(numerator, denominator)
    }
    pub const fn open_collaboration_dialog() -> Self {
        Self::OpenCollaborationDialog
    }
    pub const fn close_collaboration_dialog() -> Self {
        Self::CloseCollaborationDialog
    }
    pub const fn open_speed_change_dialog() -> Self {
        Self::OpenSpeedChangeDialog
    }
    pub const fn close_speed_change_dialog() -> Self {
        Self::CloseSpeedChangeDialog
    }
    pub const fn confirm_speed_change(factor: f32) -> Self {
        Self::ConfirmSpeedChange(factor)
    }
    pub const fn open_project_settings_dialog() -> Self {
        Self::OpenProjectSettingsDialog
    }
    pub const fn close_project_settings_dialog() -> Self {
        Self::CloseProjectSettingsDialog
    }
    pub fn apply_project_settings(title: String, tempo: f64, copyright: String) -> Self {
        Self::ApplyProjectSettings {
            title,
            tempo,
            copyright,
        }
    }
    pub fn open_load_confirm_dialog(path: String, size_mb: f64) -> Self {
        Self::OpenLoadConfirmDialog { path, size_mb }
    }
}
