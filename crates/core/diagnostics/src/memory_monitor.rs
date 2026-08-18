//! 实时内存监控模块（跨平台）
//!
//! 监控进程 RSS（Resident Set Size），确保给操作系统保留足够的空闲内存。
//! 在 RSS 接近软限制时**主动终止进程**（而非等到 OOM killer 介入），
//! 确保：
//! - 留够 OS 空闲内存（默认 512MB）
//! - `abort()` 始终能成功执行（不会因内存耗尽而失败）
//! - 留下完整的日志信息便于诊断
//!
//! # 平台支持状态
//!
//! | 平台     | 后台监控 | 看门狗 | 同步检查 |
//! |----------|---------|--------|---------|
//! | Linux    | ✅      | ✅     | ✅      |
//! | Windows  | ✅      | ✅     | ✅      |
//! | macOS    | ❌      | ❌     | ✅      |
//!
//! **macOS 上后台监控和看门狗已禁用**，原因是 macOS 的内存压力模型
//! （memory pressure + 激进 swap）与 Linux/Windows 不同，固定间隔轮询 RSS
//! 价值有限。同步检查 [`MemoryMonitor::check()`] 在所有平台上均可用手动触发。
//! 详见各函数文档中的 `TODO: macOS 内存监控`。
//!
//! # 架构
//!
//! 三层防护，独立运行：
//!
//! 1. **后台线程**（主防护）— 调用 [`spawn_monitor_thread()`] 启动独立线程，
//!    每隔 100ms 检查 RSS，超限时直接 `std::process::abort()` 终止进程。
//!    主线程即使被大任务阻塞也不受影响。
//!
//!    分级阈值（相对 soft_limit = total - 512MB OS保留）：
//!     - 🟢 < 75%  ：安全区，静默
//!     - 🟡 75-90% ：预警区，节流打印 warning
//!     - 🟠 90-95% ：紧急区，每次打印 critical 预警
//!     - 🔴 >= 95% ：**强制终止**，`std::process::abort()`（不等 100%）
//!
//! 2. **同步检查**（第二道防线）— [`MemoryMonitor::check()`] 在加载/渲染
//!    大分配前调用，`panic!()` 抛出详细内存报告，捕获突发分配高峰。
//!
//! 3. **RSS 读取容错** — 连续 3 次读取失败才跳过检查，避免偶尔的平台 API 抖动漏报。
//!
//! # 为什么 95% 就终止，不等 100%？
//!
//! 等到 RSS > soft_limit（100%）才终止，意味着只剩 512MB 可用内存。
//! 在这期间如果发生突发分配（如加载黑乐谱），可能瞬间吞掉最后 512MB，
//! 导致 `abort()` 本身因内存不足失败。在 95% 终止给终止过程自身留余量。
//!
//! # 子模块
//! - [`platform`]: 平台专属内存信息获取（Linux: /proc, macOS: sysctl/task_info, Windows: WinAPI）
//! - [`watchdog`]: 完全独立的看门狗线程（Linux: SIGKILL, macOS: SIGKILL, Windows: TerminateProcess）
//! - [`midi_guard`]: MIDI 加载状态标志（看门狗只在加载 MIDI 期间监控内存）
//! - [`background`]: 后台监控线程与看门狗启动函数（spawn_monitor_thread / spawn_all_monitors）

pub mod midi_guard;
pub mod platform;
pub mod watchdog;

mod background;
#[cfg(test)]
mod tests;

pub use background::{spawn_all_monitors, spawn_monitor_thread};

use std::sync::OnceLock;

// =============================================================================
// 常量
// =============================================================================

/// 默认保留给操作系统的内存量（字节），默认 512 MB
pub(crate) const DEFAULT_RESERVE_BYTES: u64 = 512 * 1024 * 1024;

/// 预警阈值比例（相对 soft_limit）
const WARN_THRESHOLD: f64 = 0.75;
/// 紧急预警阈值比例
const CRITICAL_THRESHOLD: f64 = 0.90;
/// 强制终止阈值比例（在 RSS 达到 soft_limit 的此比例时主动 abort/panic）
const ABORT_THRESHOLD: f64 = 0.95;
/// 连续 RSS 读取失败上限（超过此值才跳过检查）
const MAX_RSS_FAILURES: u32 = 3;

// =============================================================================
// MemoryMonitor
// =============================================================================

/// 内存监控器
///
/// 单例，全局可通过 [`MemoryMonitor::global()`] 访问。
/// 分级阈值（相对 soft_limit = total - 512MB OS保留）：
/// - 🟢 < 75%  ：安全区，静默
/// - 🟡 75-90% ：预警区，节流打印 warning
/// - 🟠 90-95% ：紧急区，每次打印 critical 预警
/// - 🔴 >= 95% ：**强制终止**，panic / abort
pub struct MemoryMonitor {
    /// 总物理内存（字节）
    total_physical: u64,
    /// 软限制 = total_physical - reserve_for_os（字节）
    soft_limit: u64,
    /// 保留给 OS 的内存（字节）
    reserve_bytes: u64,
    /// 连续 RSS 读取失败计数（超过 MAX_RSS_FAILURES 才跳过）
    rss_fail_count: std::sync::atomic::AtomicU32,
    /// 预警节流计数器（每 5 次打一次 warning）
    warn_throttle: std::sync::atomic::AtomicU32,
}

impl MemoryMonitor {
    /// 使用默认配置创建（保留 512 MB）
    fn new() -> Self {
        let total = platform::get_total_physical_memory();
        let reserve = DEFAULT_RESERVE_BYTES;
        let limit = total.saturating_sub(reserve);

        tracing::info!(
            "MemoryMonitor: 总物理内存 {} MB, 保留 {} MB, 软限制 {} MB",
            total / 1024 / 1024,
            reserve / 1024 / 1024,
            limit / 1024 / 1024,
        );

        assert!(
            limit > 0,
            "MemoryMonitor: 总物理内存 ({} MB) 小于保留量 ({} MB)，系统无法运行",
            total / 1024 / 1024,
            reserve / 1024 / 1024,
        );

        Self {
            total_physical: total,
            soft_limit: limit,
            reserve_bytes: reserve,
            rss_fail_count: std::sync::atomic::AtomicU32::new(0),
            warn_throttle: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// 获取全局单例
    pub fn global() -> &'static MemoryMonitor {
        static INSTANCE: OnceLock<MemoryMonitor> = OnceLock::new();
        INSTANCE.get_or_init(MemoryMonitor::new)
    }

    /// 获取当前 RSS
    pub fn current_rss(&self) -> u64 {
        platform::get_current_rss()
    }

    /// 获取软限制
    pub fn soft_limit(&self) -> u64 {
        self.soft_limit
    }

    /// 获取总物理内存
    pub fn total_physical(&self) -> u64 {
        self.total_physical
    }

    /// 获取当前内存使用率（0.0 ~ 1.0），/proc 不可用时返回 0.0
    pub fn usage_ratio(&self) -> f64 {
        let rss = platform::get_current_rss();
        if rss == 0 || self.total_physical == 0 {
            return 0.0;
        }
        rss as f64 / self.total_physical as f64
    }

    /// 格式化内存状态报告
    fn format_report(&self, rss: u64, label: &str) -> String {
        format!(
            "{label}\n\
             ┌─ Memory State ──────────────────────┐\n\
             │  RSS:           {:>10} MB ({:.1}%)  │\n\
             │  Total:         {:>10} MB           │\n\
             │  Soft Limit:    {:>10} MB           │\n\
             │  OS Reserve:    {:>10} MB           │\n\
             │  Warn at:       {:>10} MB (75%)     │\n\
             │  Critical at:   {:>10} MB (90%)     │\n\
             │  Abort at:      {:>10} MB (95%)     │\n\
             └─────────────────────────────────────┘",
            rss / 1024 / 1024,
            (rss as f64 / self.total_physical as f64) * 100.0,
            self.total_physical / 1024 / 1024,
            self.soft_limit / 1024 / 1024,
            self.reserve_bytes / 1024 / 1024,
            (self.soft_limit as f64 * WARN_THRESHOLD) as u64 / 1024 / 1024,
            (self.soft_limit as f64 * CRITICAL_THRESHOLD) as u64 / 1024 / 1024,
            (self.soft_limit as f64 * ABORT_THRESHOLD) as u64 / 1024 / 1024,
        )
    }

    /// 内部检查逻辑（供后台线程和同步 check 共用）
    ///
    /// 返回值：
    /// - `None` = 无法读取 RSS
    /// - `Some(false)` = 未超限
    /// - `Some(true)` = 已超限，调用者需处理
    fn check_inner(&self, log_prefix: &str) -> Option<bool> {
        let rss = platform::get_current_rss();

        // ── RSS 读取容错 ──
        if rss == 0 {
            let fail_count = self
                .rss_fail_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            if fail_count == 1 {
                tracing::warn!(
                    "{log_prefix}RSS 读取失败，后续失败将被压制 {MAX_RSS_FAILURES} 次后跳过检查"
                );
            }
            if fail_count >= MAX_RSS_FAILURES {
                tracing::error!("{log_prefix}RSS 连续 {fail_count} 次读取失败，跳过本轮检查");
                return None;
            }
            return Some(false);
        }
        self.rss_fail_count
            .store(0, std::sync::atomic::Ordering::Relaxed);

        // ── 分级阈值判断 ──
        let ratio = rss as f64 / self.soft_limit as f64;

        if ratio >= ABORT_THRESHOLD {
            tracing::error!("{}", self.format_report(rss, "MEMORY_ABORT 🔴"));
            Some(true)
        } else if ratio >= CRITICAL_THRESHOLD {
            tracing::warn!("{}", self.format_report(rss, "MEMORY_CRITICAL 🟠"));
            Some(false)
        } else if ratio >= WARN_THRESHOLD {
            let throttle = self
                .warn_throttle
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if throttle.is_multiple_of(5) {
                tracing::warn!("{}", self.format_report(rss, "MEMORY_WARNING 🟡"));
            }
            Some(false)
        } else {
            self.warn_throttle
                .store(0, std::sync::atomic::Ordering::Relaxed);
            Some(false)
        }
    }

    /// 同步检查内存限制
    ///
    /// 用于大分配前（渲染管线、MIDI 加载等）主动检查。
    /// 在 RSS >= 95% soft_limit 时 `panic!()`，不等 OOM 杀手出手。
    ///
    /// # Panics
    /// 当 RSS >= ABORT_THRESHOLD (95% of soft_limit) 时 panic
    pub fn check(&self) {
        if let Some(true) = self.check_inner("sync: ") {
            let rss = platform::get_current_rss();
            panic!(
                "{}",
                self.format_report(rss, "MEMORY_LIMIT: 进程将在 OOM 前终止以保护系统稳定 🔴")
            );
        }
    }

    /// 同步检查 + 返回是否超限（不 panic）
    pub fn is_over_limit(&self) -> bool {
        matches!(self.check_inner(""), Some(true))
    }
}
