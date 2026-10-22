//! GPU 兼容性检测 UI 状态。
//!
//! 与 lumino-gfx 的报告类型解耦：Runner 负责把 gfx 报告投影为
//! 「结论分类 + 纯文本详情」，UI 只消费这两项，不依赖图形栈类型。

pub use lumino_message::events::window::GpuCheckOutcome;

/// GPU 兼容性检测结果（UI 展示用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuCheckUiResult {
    /// 检测结论分类（决定文案与按钮可用性）
    pub outcome: GpuCheckOutcome,
    /// 技术详情（多行文本，可直接复制）
    pub detail: String,
}

impl GpuCheckUiResult {
    /// 是否判定为通过（唯一判定入口，避免各 UI 处自行比较枚举变体）
    pub fn passed(&self) -> bool {
        self.outcome.is_passed()
    }
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
