//! GPU 兼容性检测 UI 状态。
//!
//! 与 lumino-gfx 的报告类型解耦：Runner 负责把 gfx 报告转换为纯文本详情，
//! UI 只消费 `(passed, detail)`，不依赖图形栈类型。

/// GPU 兼容性检测结果（UI 展示用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuCheckUiResult {
    /// 是否通过
    pub passed: bool,
    /// 技术详情（多行文本，可直接复制）
    pub detail: String,
}

/// GPU 兼容性检测 UI 状态
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GpuCheckUiState {
    /// 尚未检查
    #[default]
    Idle,
    /// 检查进行中
    Running,
    /// 已完成（含结果）
    Done(GpuCheckUiResult),
}
