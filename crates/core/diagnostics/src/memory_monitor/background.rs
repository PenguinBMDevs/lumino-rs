//! 后台内存监控与看门狗启动
//!
//! 提供独立于主线程运行的后台内存监控线程（[`spawn_monitor_thread`]）与
//! 全量监控启动（[`spawn_all_monitors`]，含看门狗），与 [`super::MemoryMonitor`]
//! 的同步检查（check / is_over_limit）构成三层防护。
//! 拆分自 `memory_monitor.rs`。

use std::sync::OnceLock;

use super::MemoryMonitor;
use super::watchdog;

/// 后台监控线程检查间隔（毫秒）
///
/// 只在非 macOS 平台使用：macOS 上后台监控被禁用（见 [`spawn_monitor_thread()`]），
/// 编译该平台的实现会触发 dead-code 警告。
#[cfg(not(target_os = "macos"))]
const MONITOR_INTERVAL_MS: u64 = 100;

/// 调用 abort（直接写 stderr，不依赖 tracing）
#[cfg(not(target_os = "macos"))]
fn abort_process(report: &str) -> ! {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{}", report);
    let _ = std::io::stderr().flush();
    std::process::abort()
}

/// 启动后台内存监控线程
///
/// 该线程独立于主线程运行，每 100ms 检查一次 RSS。
/// 即使主线程被大任务阻塞，也能及时检测到 OOM 并终止进程。
///
/// 返回 `true` 表示线程已启动（或之前已启动），`false` 表示创建线程失败。
///
/// ## 平台说明
/// - **Linux / Windows**: 正常启动后台监控线程
/// - **macOS**: 禁用（见 [`spawn_all_monitors()`] 说明）
#[cfg(not(target_os = "macos"))]
pub fn spawn_monitor_thread() -> bool {
    static SPAWNED: OnceLock<std::thread::JoinHandle<()>> = OnceLock::new();

    if SPAWNED.get().is_some() {
        return true;
    }

    match std::thread::Builder::new()
        .name("memory-monitor".into())
        .spawn(|| {
            let monitor = MemoryMonitor::global();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(MONITOR_INTERVAL_MS));

                match monitor.check_inner("bg: ") {
                    Some(true) => {
                        abort_process("MemoryMonitor: 内存已达 95% 软限制，主动终止以保护系统稳定");
                    }
                    None => {}
                    Some(false) => {}
                }
            }
        }) {
        Ok(handle) => {
            let _ = SPAWNED.set(handle);
            tracing::info!(
                "MemoryMonitor: 后台监控线程已启动 (间隔={}ms)",
                MONITOR_INTERVAL_MS
            );
            true
        }
        Err(e) => {
            tracing::error!("MemoryMonitor: 无法创建后台监控线程: {e}");
            false
        }
    }
}

/// macOS 上禁用后台内存监控线程
///
/// TODO: macOS 内存监控 — 当前 macOS 上禁用了后台内存监控线程和看门狗，
/// 因为 macOS 的内存压力模型与 Linux/Windows 不同（memory pressure + swap 机制更激进），
/// 且 macOS 的 `task_info` 在后台线程频繁调用时可能引入不必要的性能抖动。
/// 后续方案：使用 `dispatch_source` 监听 macOS 的 `memorypressure` 事件，
/// 仅在系统内存压力升高时触发检查，而非固定间隔轮询。
#[cfg(target_os = "macos")]
pub fn spawn_monitor_thread() -> bool {
    tracing::info!("MemoryMonitor: macOS 上禁用后台内存监控线程（参见 spawn_all_monitors 文档）");
    true
}

/// 同时启动主监控和看门狗
///
/// 等价于依次调用 `spawn_monitor_thread()` + [`watchdog::spawn_watchdog()`]，
/// 确保两层防线同时就位。
///
/// 返回 `true` 表示所有启用的监控均已成功启动。
///
/// ## 平台说明
/// - **Linux / Windows**: 正常启动后台监控线程 + 看门狗
/// - **macOS**: 两者均禁用。原因：
///   1. macOS 的内存压力模型（memory pressure + 激进 swap）与 Linux/Windows 不同，
///      固定间隔轮询 RSS 的价值有限，反而可能因 `task_info` 频繁调用引入性能抖动。
///   2. macOS 的 `SIGKILL` 行为与 Linux 一致，但看门狗的轮询模型在 macOS 上
///      与系统自身的压力管理机制重叠，收益不大。
///   3. 同步检查 [`MemoryMonitor::check()`] 仍可用——大分配前手动调用。
///   4. 详见 TODO: macOS 内存监控 — 后续改用 `dispatch_source` 监听 memorypressure 事件。
#[cfg(not(target_os = "macos"))]
pub fn spawn_all_monitors() -> bool {
    spawn_monitor_thread() && watchdog::spawn_watchdog()
}

/// macOS 上禁用所有内存监控（包括看门狗）
///
/// TODO: macOS 内存监控 — 当前 macOS 上禁用了后台内存监控线程和看门狗。
/// 后续方案：使用 `dispatch_source` 监听 macOS 的 `memorypressure` 事件，
/// 仅在系统内存压力升高时触发检查，而非固定间隔轮询。
#[cfg(target_os = "macos")]
pub fn spawn_all_monitors() -> bool {
    tracing::info!(
        "MemoryMonitor: macOS 上禁用后台内存监控和看门狗（参见 spawn_all_monitors 文档）"
    );
    true
}
