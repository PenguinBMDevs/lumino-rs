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

/// 手动检测的重试门控。
///
/// 与 [`GpuCheckUiState`] **正交**：检测结果照常展示，门控只决定按钮能否点击。
/// 由 Runner 会话状态机产出并注入；设置面板只读，不自行推断。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GpuCheckRetryGate {
    /// 可重试
    #[default]
    Ready,
    /// 超时后的短时冷却（避免在疑似挂起的驱动上立刻叠加第二个检测）
    Cooling,
    /// 连续超时后锁定到应用重启（每次超时都会遗留无法回收的检测线程与实例）
    Locked,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `passed()` 必须严格由结论派生（UI 不得再存第二个「是否通过」字段）
    #[test]
    fn test_ui_result_passed_follows_outcome() {
        let passed = GpuCheckUiResult {
            outcome: GpuCheckOutcome::Passed,
            detail: String::new(),
        };
        assert!(passed.passed());

        for outcome in [
            GpuCheckOutcome::NoAdapter,
            GpuCheckOutcome::DeviceRequestFailed,
            GpuCheckOutcome::RenderFailed,
            GpuCheckOutcome::TimedOut,
            GpuCheckOutcome::Unknown,
        ] {
            let failed = GpuCheckUiResult {
                outcome,
                detail: String::new(),
            };
            assert!(!failed.passed(), "{outcome:?} 不得显示为通过");
        }
    }

    /// 默认门控必须是可重试：面板初始状态不得意外禁用按钮
    #[test]
    fn test_retry_gate_defaults_to_ready() {
        assert_eq!(GpuCheckRetryGate::default(), GpuCheckRetryGate::Ready);
    }
}
