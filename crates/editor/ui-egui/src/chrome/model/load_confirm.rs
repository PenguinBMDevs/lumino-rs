//! 加载确认对话框模型。

/// 加载确认：文件信息三行 / 提示 / 两个按钮。
#[derive(Debug, Clone, Default)]
pub struct LoadConfirmModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 「文件: {name}」（14px）。
    pub file_line: String,
    /// 「大小: {size} MB」（14px）。
    pub size_line: String,
    /// 「路径: {path}」（12px 弱色）。
    pub path_line: String,
    /// 「此文件较大，加载可能需要一些时间…」（13px 弱色）。
    pub hint: String,
    /// 取消按钮。
    pub cancel: String,
    /// 加载按钮。
    pub confirm: String,
}
