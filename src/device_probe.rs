//! GPU 兼容性检测的子进程隔离。
//!
//! # 为什么需要它
//!
//! 进程内的检测线程一旦在驱动调用（loader / ICD / 队列提交）里挂起，`recv_timeout`
//! 只能「放弃等待」——Rust 没有线程取消，线程会 detach 并**永久**持有驱动锁，
//! 把主渲染的提交 / present 一起拖死，表现为整个 UI 卡死。唯一能真正收回资源的
//! 手段，是把检测搬到**可强杀的执行边界**：子进程。
//!
//! # 结构
//!
//! - [`wire`]：IPC 数据契约（带协议版本；与 gfx 内部类型解耦）；
//! - [`child`]：子进程侧（看门狗 + 原子写结果）；
//! - [`supervisor`]：父进程侧（启动 / 超时强杀 / 收割 / 解析 / Job Object）。
//!
//! # 平台口径（卡片显式决策）
//!
//! Windows 走子进程（根治）；macOS / Linux 本期沿用进程内线程模型 + 阶段一兜底
//! （结果区分 / 冷却 / 会话锁定）。`supervisor` 本身是跨平台 std 代码，后续测量
//! 冷启动开销后可以把 [`run_check_with_timeout`] 的非 Windows 分支切过来，
//! 一并删除线程模型与 detach 注释。
//!
//! # 失败语义（fail closed）
//!
//! 子进程无法启动 / 崩溃 / 回传不可解析 → 一律 [`GpuCheckReport`] 失败并标
//! `Aborted`，**绝不静默降级回进程内检测**：降级等于把已知会冻死 UI 的路径重新打开。

mod child;
mod supervisor;
mod wire;

use std::time::Duration;

#[cfg(not(windows))]
use lumino_gfx::device_check;
use lumino_gfx::device_check::{FallbackDiagnostics, GpuCheckReport};

/// 子进程模式标记（父进程用该参数重新拉起自身）
pub(crate) const CHILD_FLAG: &str = "--device-check-probe";
/// 回退后端诊断策略参数
pub(crate) const FALLBACK_FLAG: &str = "--device-check-fallback";
/// 语义超时参数（毫秒）
pub(crate) const TIMEOUT_FLAG: &str = "--device-check-timeout-ms";

/// 是否为子进程探针模式。
///
/// 必须在 `logging::init()`、puffin 服务器、内存监控与 `clap` 解析**之前**判定：
/// 否则子进程会抢 8585 端口、起看门狗线程、往 stdout 灌日志，甚至被 clap 以
/// 「未知参数」退出码 2 拒绝。
pub(crate) fn is_child_mode() -> bool {
    std::env::args_os().skip(1).any(|arg| arg == CHILD_FLAG)
}

/// 子进程入口：执行检测 → 写结果 → 退出。**不返回。**
pub(crate) fn run_child_mode() -> ! {
    child::run()
}

/// 带超时的检测入口（Windows 走子进程，其他平台沿用线程模型）。
pub(crate) fn run_check_with_timeout(
    timeout: Duration,
    when: FallbackDiagnostics,
) -> GpuCheckReport {
    #[cfg(windows)]
    {
        supervisor::run(timeout, when)
    }
    #[cfg(not(windows))]
    {
        // 卡片范围决策：macOS/Linux 本期保留线程模型（阶段一兜底为结果区分 + 冷却 +
        // 会话锁定）。supervisor 已按跨平台实现并有单测覆盖，切换只需改这一行。
        device_check::run_check_with_timeout(timeout, when)
    }
}

/// 回退诊断策略 → 子进程命令行取值
pub(crate) fn fallback_arg(when: FallbackDiagnostics) -> &'static str {
    match when {
        FallbackDiagnostics::Never => "never",
        FallbackDiagnostics::OnFailure => "on-failure",
        FallbackDiagnostics::Always => "always",
    }
}

/// 子进程命令行取值 → 回退诊断策略（未知值返回 `None`，由调用方按协议错误处理）
pub(crate) fn parse_fallback(raw: &str) -> Option<FallbackDiagnostics> {
    match raw {
        "never" => Some(FallbackDiagnostics::Never),
        "on-failure" => Some(FallbackDiagnostics::OnFailure),
        "always" => Some(FallbackDiagnostics::Always),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 父子两端的策略编码必须完全一致（写端改了忘改读端 = 静默降级成默认策略）
    #[test]
    fn test_fallback_policy_round_trips() {
        for when in [
            FallbackDiagnostics::Never,
            FallbackDiagnostics::OnFailure,
            FallbackDiagnostics::Always,
        ] {
            assert_eq!(
                parse_fallback(fallback_arg(when)),
                Some(when),
                "策略 {when:?} 的编码/解码不一致"
            );
        }
    }

    #[test]
    fn test_unknown_fallback_value_is_rejected() {
        assert_eq!(parse_fallback(""), None);
        assert_eq!(parse_fallback("Always"), None);
        assert_eq!(parse_fallback("on_failure"), None);
    }
}
