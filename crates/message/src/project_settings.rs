//! 工程设置对话框动作

/// 工程设置对话框动作
#[derive(Debug, Clone)]
pub enum ProjectSettingsAction {
    /// 打开对话框
    OpenDialog,
    /// 关闭对话框
    CloseDialog,
    /// 确认工程设置
    Confirm,
    /// 项目名称变更
    TitleChanged(String),
    /// BPM 速度变更
    TempoChanged(String),
    /// 版权信息变更
    CopyrightChanged(String),
    /// 作者变更
    AuthorChanged(String),
    /// 拍号分子变更
    TimeSignatureNumeratorChanged(String),
    /// 拍号分母变更
    TimeSignatureDenominatorChanged(String),
}
