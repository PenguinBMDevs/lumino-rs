#[derive(Debug, Clone)]
/// 窗口事件
pub enum Event {
    Drag,
    Close,
    ToggleMaximize,
    Maximize,
    Minimize,
    /// 打开自定义精度对话框窗口
    OpenCustomPrecisionDialog,
    /// 关闭自定义精度对话框窗口
    CloseCustomPrecisionDialog,
    /// 应用自定义精度设置 (numerator, denominator)
    ApplyCustomPrecision(u32, u32),
}
