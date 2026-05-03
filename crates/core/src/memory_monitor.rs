//! 实时内存监控模块
//!
//! 监控进程 RSS（Resident Set Size），确保给操作系统保留足够的空闲内存。
//! 在 RSS 接近软限制时**主动终止进程**（而非等到 OOM killer 介入），
//! 确保：
//! - 留够 OS 空闲内存（默认 512MB）
//! - `abort()` 始终能成功执行（不会因内存耗尽而失败）
//! - 留下完整的日志信息便于诊断
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
//! 3. **RSS 读取容错** — 连续 3 次读取失败才跳过检查，避免偶尔的 /proc 抖动漏报。
//!
//! # 为什么 95% 就终止，不等 100%？
//!
//! 等到 RSS > soft_limit（100%）才终止，意味着只剩 512MB 可用内存。
//! 在这期间如果发生突发分配（如加载黑乐谱），可能瞬间吞掉最后 512MB，
//! 导致 `abort()` 本身因内存不足失败。在 95% 终止给终止过程自身留余量。
//!
//! # 示例
//!
//! ```ignore
//! // 在 main() 中启动后台监控线程
//! lumino_core::memory_monitor::spawn_monitor_thread();
//!
//! // 在可能大分配的位置前同步检查
//! lumino_core::memory_monitor::MemoryMonitor::global().check();
//! ```

use std::sync::OnceLock;

// =============================================================================
// 常量
// =============================================================================

/// 默认保留给操作系统的内存量（字节），默认 512 MB
const DEFAULT_RESERVE_BYTES: u64 = 512 * 1024 * 1024;

/// 检测失败时的兜底总内存（8 GB）
const FALLBACK_TOTAL_MEMORY: u64 = 8 * 1024 * 1024 * 1024;

/// 预警阈值比例（相对 soft_limit）
const WARN_THRESHOLD: f64 = 0.75;
/// 紧急预警阈值比例
const CRITICAL_THRESHOLD: f64 = 0.90;
/// 强制终止阈值比例（在 RSS 达到 soft_limit 的此比例时主动 abort/panic）
/// 不等 100% 才终止，给 abort() 自身留内存余量
const ABORT_THRESHOLD: f64 = 0.95;
/// 连续 RSS 读取失败上限（超过此值才跳过检查）
const MAX_RSS_FAILURES: u32 = 3;

// =============================================================================
// 平台专属：获取总物理内存
// =============================================================================

#[cfg(target_os = "linux")]
fn get_total_physical_memory() -> u64 {
    // Linux: 读取 /proc/meminfo 的 MemTotal
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("MemoryMonitor: 无法读取 /proc/meminfo ({}), 使用兜底值", e);
            return FALLBACK_TOTAL_MEMORY;
        }
    };

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2
                && let Ok(kb) = parts[0].parse::<u64>()
            {
                return kb * 1024; // kB → bytes
            }
        }
    }

    tracing::warn!("MemoryMonitor: /proc/meminfo 中未找到 MemTotal, 使用兜底值");
    FALLBACK_TOTAL_MEMORY
}

#[cfg(target_os = "macos")]
fn get_total_physical_memory() -> u64 {
    // macOS: 使用 libc::sysctl CTL_HW + HW_MEMSIZE
    let mut mib: [libc::c_int; 2] = [libc::CTL_HW, libc::HW_MEMSIZE];
    let mut size: u64 = 0;
    let mut len = std::mem::size_of::<u64>() as libc::size_t;

    let ret = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            &mut size as *mut u64 as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };

    if ret == 0 && len == std::mem::size_of::<u64>() {
        size
    } else {
        tracing::warn!("MemoryMonitor: sysctl HW_MEMSIZE 失败 (ret={ret}), 使用兜底值");
        FALLBACK_TOTAL_MEMORY
    }
}

#[cfg(target_os = "windows")]
fn get_total_physical_memory() -> u64 {
    // Windows: 使用 GlobalMemoryStatusEx
    unsafe {
        let mut statex: winapi::um::sysinfoapi::MEMORYSTATUSEX = std::mem::zeroed();
        statex.dwLength = std::mem::size_of::<winapi::um::sysinfoapi::MEMORYSTATUSEX>() as u32;
        if winapi::um::sysinfoapi::GlobalMemoryStatusEx(&mut statex) != 0 {
            statex.ullTotalPhys
        } else {
            tracing::warn!("MemoryMonitor: GlobalMemoryStatusEx 失败, 使用兜底值");
            FALLBACK_TOTAL_MEMORY
        }
    }
}

// =============================================================================
// 平台专属：获取当前 RSS
// =============================================================================

#[cfg(target_os = "linux")]
fn get_current_rss() -> u64 {
    // Linux: 读取 /proc/self/status 的 VmRSS
    let content = match std::fs::read_to_string("/proc/self/status") {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("MemoryMonitor: 无法读取 /proc/self/status ({e}), 跳过检查");
            return 0;
        }
    };

    for line in content.lines() {
        // VmRSS:   123456 kB
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2
                && let Ok(kb) = parts[0].parse::<u64>()
            {
                return kb * 1024; // kB → bytes
            }
        }
    }

    tracing::warn!("MemoryMonitor: /proc/self/status 中未找到 VmRSS");
    0
}

#[cfg(target_os = "macos")]
fn get_current_rss() -> u64 {
    // macOS: 使用 libc::task_info 获取 mach_task_basic_info.resident_size
    // MACH_TASK_BASIC_INFO flavor = 20
    const MACH_TASK_BASIC_INFO: u32 = 20;
    const KERN_SUCCESS: i32 = 0;

    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
    }

    let mut info = std::mem::MaybeUninit::<MachTaskBasicInfo>::uninit();
    let mut count = (std::mem::size_of::<MachTaskBasicInfo>() / std::mem::size_of::<u32>()) as u32;

    let result = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            MACH_TASK_BASIC_INFO,
            info.as_mut_ptr() as libc::task_info_t,
            &mut count,
        )
    };

    if result == KERN_SUCCESS {
        unsafe { (*info.as_ptr()).resident_size }
    } else {
        tracing::warn!("MemoryMonitor: task_info 失败 (result={result}), 跳过检查");
        0
    }
}

#[cfg(target_os = "windows")]
fn get_current_rss() -> u64 {
    // Windows: 使用 GetProcessMemoryInfo 获取 WorkingSetSize
    unsafe {
        let mut pmc: winapi::um::psapi::PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<winapi::um::psapi::PROCESS_MEMORY_COUNTERS>() as u32;
        if winapi::um::psapi::GetProcessMemoryInfo(
            winapi::um::processthreadsapi::GetCurrentProcess(),
            &mut pmc,
            pmc.cb,
        ) != 0
        {
            pmc.WorkingSetSize as u64
        } else {
            tracing::warn!("MemoryMonitor: GetProcessMemoryInfo 失败, 跳过检查");
            0
        }
    }
}

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
        let total = get_total_physical_memory();
        let reserve = DEFAULT_RESERVE_BYTES;
        let limit = total.saturating_sub(reserve);

        tracing::info!(
            "MemoryMonitor: 总物理内存 {} MB, 保留 {} MB, 软限制 {} MB",
            total / 1024 / 1024,
            reserve / 1024 / 1024,
            limit / 1024 / 1024,
        );

        // 如果总内存连保留量都不够，直接 panic
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
        get_current_rss()
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
        let rss = get_current_rss();
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
        let rss = get_current_rss();

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
        // 阈值基于 soft_limit（= total - 512MB OS保留），不是 total。
        // 所以 95% soft_limit ≈ total 剩余 ~537MB，abort 有足够余量。
        let ratio = rss as f64 / self.soft_limit as f64;

        if ratio >= ABORT_THRESHOLD {
            // 🔴 强制终止区（>= 95% soft_limit）
            // 不等 RSS > soft_limit（100%）才动手，因为突发分配可能瞬间
            // 吞掉最后 512MB 导致 abort() 本身失败。
            tracing::error!("{}", self.format_report(rss, "MEMORY_ABORT 🔴"));
            Some(true)
        } else if ratio >= CRITICAL_THRESHOLD {
            // 🟠 紧急区（90-95%）→ 每次打印 critical 预警
            tracing::warn!("{}", self.format_report(rss, "MEMORY_CRITICAL 🟠"));
            Some(false)
        } else if ratio >= WARN_THRESHOLD {
            // 🟡 预警区（75-90%）→ 节流打印
            let throttle = self
                .warn_throttle
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if throttle % 5 == 0 {
                tracing::warn!("{}", self.format_report(rss, "MEMORY_WARNING 🟡"));
            }
            Some(false)
        } else {
            // 🟢 安全区
            self.warn_throttle
                .store(0, std::sync::atomic::Ordering::Relaxed);
            Some(false)
        }
    }

    /// 同步检查内存限制
    ///
    /// 用于大分配前（渲染管线、MIDI 加载等）主动检查。
    /// 在 RSS >= 95% soft_limit 时 `panic!()`，不等 OOM 杀手出手。
    /// 相比后台线程的 `abort()`，`panic!()` 会触发 panic hook，
    /// 留下更完整的诊断信息（backtrace）。
    ///
    /// # Panics
    /// 当 RSS >= ABORT_THRESHOLD (95% of soft_limit) 时 panic，
    /// 附带详细内存状态信息
    pub fn check(&self) {
        if let Some(true) = self.check_inner("sync: ") {
            let rss = get_current_rss();
            panic!(
                "{}",
                self.format_report(rss, "MEMORY_LIMIT: 进程将在 OOM 前终止以保护系统稳定 🔴",)
            );
        }
    }

    /// 同步检查 + 返回是否超限（不 panic）
    pub fn is_over_limit(&self) -> bool {
        matches!(self.check_inner(""), Some(true))
    }
}

/// 后台监控线程检查间隔（毫秒）
/// 从 200ms 降到 100ms：突发分配（如打开黑乐谱）能在 1-2 轮内捕获，
/// 线程开销可忽略（只读 /proc/self/status 一行即返回）。
const MONITOR_INTERVAL_MS: u64 = 100;

/// 调用 abort（直接写 stderr，不依赖 tracing）
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
/// 分级阈值（相对 soft_limit = total - 512MB OS保留）：
/// - 🟢 < 75%  ：安全区，静默
/// - 🟡 75-90% ：预警区，节流打印 warning
/// - 🟠 90-95% ：紧急区，每次打印 critical 预警
/// - 🔴 >= 95% ：`std::process::abort()` 强制终止（不等 100%）
///
/// RSS 读取出错时：连续 3 次失败后才跳过，避免 /proc 偶尔抖动导致漏报。
pub fn spawn_monitor_thread() {
    static SPAWNED: std::sync::OnceLock<std::thread::JoinHandle<()>> = OnceLock::new();

    SPAWNED.get_or_init(|| {
        let handle = std::thread::Builder::new()
            .name("memory-monitor".into())
            .spawn(|| {
                let monitor = MemoryMonitor::global();
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(MONITOR_INTERVAL_MS));

                    match monitor.check_inner("bg: ") {
                        // 🔴 RSS >= 95% soft_limit → abort 整个进程（不等 100%）
                        Some(true) => {
                            abort_process(
                                "MemoryMonitor: 内存已达 95% 软限制，主动终止以保护系统稳定",
                            );
                        }
                        // RSS 无法读取 → 跳过（已由 check_inner 记录日志）
                        None => {}
                        // 🟢 🟡 🟠 未超限 → 继续
                        Some(false) => {}
                    }
                }
            })
            .expect("MemoryMonitor: 无法创建后台监控线程");

        tracing::info!(
            "MemoryMonitor: 后台监控线程已启动 (间隔={}ms)",
            MONITOR_INTERVAL_MS
        );

        handle
    });
}

// =============================================================================
// 看门狗（Watchdog）— 完全独立的终极防线
//
// 架构独立性：
// - 启动时捕获主进程 PID，用 PID 构造 /proc/{pid}/status（不碰 /proc/self）
// - 自己的内存读取函数（watchdog_get_*），不调用 MemoryMonitor 的任何方法
// - 自己的阈值逻辑（RSS > 100% soft_limit OR 系统可用 < 350MB）
// - 自己的终止手段（SIGKILL / TerminateProcess — 不可阻塞/忽略）
//
// 时序：
// - 主监控在 95% soft_limit abort，触发时有 backtrace
// - 看门狗在 100% soft_limit 或系统内存 ≤ 350MB 时 SIGKILL
// - 50ms 轮询确保在突发分配瞬间吞掉最后 350MB 之前拦截
// =============================================================================

/// 看门狗检查间隔（毫秒）— 50ms，只读 /proc 一行，零 CPU 开销
const WATCHDOG_INTERVAL_MS: u64 = 50;

/// 看门狗系统可用内存阈值：低于此值时直接 SIGKILL
/// 设 350MB 确保即使主监控 95% 阈值失效（如 panic hook 卡死），
/// 看门狗也能在系统彻底无响应前拦截。
const WATCHDOG_MIN_AVAILABLE_BYTES: u64 = 350 * 1024 * 1024;

// ── 平台相关：通过 PID 获取进程 RSS（完全独立于 /proc/self）──

#[cfg(target_os = "linux")]
fn watchdog_get_process_rss(pid: u32) -> u64 {
    // 用 PID 构造路径，绝不使用 /proc/self（自引用对看门狗不够独立）
    let path = format!("/proc/{pid}/status");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2
                && let Ok(kb) = parts[0].parse::<u64>()
            {
                return kb * 1024;
            }
        }
    }
    0
}

#[cfg(target_os = "macos")]
fn watchdog_get_process_rss(_pid: u32) -> u64 {
    // macOS 下跨进程读取 RSS 需 task_for_pid（需 entitlement），
    // 但看门狗与主监控在同一进程，用 task_info 读自身 RSS 已足够。
    // 关键区别在于终止手段：SIGKILL vs abort()。
    get_current_rss()
}

#[cfg(target_os = "windows")]
fn watchdog_get_process_rss(pid: u32) -> u64 {
    unsafe {
        let handle = winapi::um::processthreadsapi::OpenProcess(
            winapi::um::winnt::PROCESS_QUERY_INFORMATION | winapi::um::winnt::PROCESS_VM_READ,
            0,
            pid,
        );
        if handle.is_null() {
            return 0;
        }
        let mut pmc: winapi::um::psapi::PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<winapi::um::psapi::PROCESS_MEMORY_COUNTERS>() as u32;
        let result = winapi::um::psapi::GetProcessMemoryInfo(handle, &mut pmc, pmc.cb);
        winapi::um::handleapi::CloseHandle(handle);
        if result != 0 {
            pmc.WorkingSetSize as u64
        } else {
            0
        }
    }
}

// ── 平台相关：获取系统可用内存 ──

#[cfg(target_os = "linux")]
fn watchdog_get_available_memory() -> u64 {
    // Linux: /proc/meminfo MemAvailable（比 MemFree 更准确，包含可回收缓存）
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return u64::MAX,
    };
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2
                && let Ok(kb) = parts[0].parse::<u64>()
            {
                return kb * 1024;
            }
        }
    }
    u64::MAX
}

#[cfg(target_os = "macos")]
fn watchdog_get_available_memory() -> u64 {
    // macOS: 使用 host_statistics64 获取 vm_stat 计算可用内存
    // 简化处理：返回 u64::MAX（不做精确计算），主要依赖 RSS 阈值触发
    // TODO: 用 host_statistics64(HOST_VM_INFO64) 读取 free + inactive 页数
    u64::MAX
}

#[cfg(target_os = "windows")]
fn watchdog_get_available_memory() -> u64 {
    unsafe {
        let mut statex: winapi::um::sysinfoapi::MEMORYSTATUSEX = std::mem::zeroed();
        statex.dwLength = std::mem::size_of::<winapi::um::sysinfoapi::MEMORYSTATUSEX>() as u32;
        if winapi::um::sysinfoapi::GlobalMemoryStatusEx(&mut statex) != 0 {
            statex.ullAvailPhys
        } else {
            u64::MAX
        }
    }
}

// ── 平台相关：强制终止进程 ──

#[cfg(unix)]
fn watchdog_force_kill(pid: u32) {
    // SIGKILL (signal 9) — 不可被捕获、阻塞、忽略，是最后的核选项
    let ret = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    if ret != 0 {
        // kill 失败（理论上不应当，我们就是自己），用 abort 兜底
        std::process::abort();
    }
}

#[cfg(windows)]
fn watchdog_force_kill(pid: u32) {
    unsafe {
        let handle = winapi::um::processthreadsapi::OpenProcess(
            winapi::um::winnt::PROCESS_TERMINATE,
            0,
            pid,
        );
        if handle.is_null() {
            std::process::abort();
        }
        winapi::um::processthreadsapi::TerminateProcess(handle, 1);
        winapi::um::handleapi::CloseHandle(handle);
    }
}

/// 启动看门狗线程（完全独立的终极防线）
///
/// 与 `spawn_monitor_thread()` 完全独立运行：
/// - 自己的 PID 引用（启动时捕获，用 /proc/{pid} 而非 /proc/self）
/// - 自己的内存读取函数（watchdog_get_*），零依赖 MemoryMonitor
/// - 自己的阈值（soft_limit 100%，主监控在 95% 已动作，看门狗兜底）
/// - 系统可用内存阈值（< 350MB 时即使 RSS 未超限也触发）
/// - 自己的终止手段（SIGKILL，不可被捕获/阻塞/忽略）
///
/// 触发条件（任一满足即 SIGKILL）：
/// 1. 进程 RSS > soft_limit（= total - 512MB，100% 阈值）
/// 2. 系统可用内存 < 350MB（防止系统级 OOM）
pub fn spawn_watchdog() {
    static SPAWNED: std::sync::OnceLock<std::thread::JoinHandle<()>> = OnceLock::new();

    SPAWNED.get_or_init(|| {
        // ── 启动时独立获取 PID 和软限制（不碰 MemoryMonitor 单例）──
        let pid = std::process::id();
        let total = get_total_physical_memory();
        let soft_limit = total.saturating_sub(DEFAULT_RESERVE_BYTES);

        tracing::info!(
            "MemoryWatchdog: 启动 (PID={}, soft_limit={} MB, sys_available_min={} MB, poll={}ms)",
            pid,
            soft_limit / 1024 / 1024,
            WATCHDOG_MIN_AVAILABLE_BYTES / 1024 / 1024,
            WATCHDOG_INTERVAL_MS,
        );

        let handle = std::thread::Builder::new()
            .name("memory-watchdog".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(WATCHDOG_INTERVAL_MS));

                    // 1. 独立读取进程 RSS（Linux 用 /proc/{pid}，非 /proc/self）
                    let rss = watchdog_get_process_rss(pid);

                    // 2. 独立读取系统可用内存
                    let available = watchdog_get_available_memory();

                    // 3. 判断触发条件
                    let rss_over_limit = rss > 0 && rss > soft_limit;
                    let system_critical = available < WATCHDOG_MIN_AVAILABLE_BYTES;

                    if rss_over_limit || system_critical {
                        // 写 stderr 记录（不能依赖 tracing，tracing 可能已挂）
                        use std::io::Write;
                        let trigger_reason = if rss_over_limit {
                            format!(
                                "RSS {}MB > soft_limit {}MB",
                                rss / 1024 / 1024,
                                soft_limit / 1024 / 1024,
                            )
                        } else {
                            format!(
                                "SysAvailable {}MB < {}MB",
                                available / 1024 / 1024,
                                WATCHDOG_MIN_AVAILABLE_BYTES / 1024 / 1024,
                            )
                        };
                        let _ = writeln!(
                            std::io::stderr(),
                            "\n\
                             ┌─ WATCHDOG TRIGGERED ──────────────────────┐\n\
                             │  PID:          {:>10}                    │\n\
                             │  RSS:           {:>10} MB                  │\n\
                             │  Soft Limit:    {:>10} MB                  │\n\
                             │  Sys Available: {:>10} MB                  │\n\
                             │  Reason:        {:<26} │\n\
                             │  Action:        SIGKILL sent              │\n\
                             └───────────────────────────────────────────┘\n",
                            pid,
                            rss / 1024 / 1024,
                            soft_limit / 1024 / 1024,
                            available / 1024 / 1024,
                            trigger_reason,
                        );
                        let _ = std::io::stderr().flush();

                        // SIGKILL — 不可被捕获/阻塞/忽略
                        watchdog_force_kill(pid);

                        // 理论上 SIGKILL 从不返回，但如果有权限问题，abort 兜底
                        std::process::abort();
                    }
                }
            })
            .expect("MemoryWatchdog: 无法创建看门狗线程");

        tracing::info!(
            "MemoryWatchdog: 看门狗线程已启动 (PID={}, interval={}ms, sys_available_min={}MB)",
            pid,
            WATCHDOG_INTERVAL_MS,
            WATCHDOG_MIN_AVAILABLE_BYTES / 1024 / 1024,
        );

        handle
    });
}

/// 同时启动主监控和看门狗
///
/// 等价于依次调用 `spawn_monitor_thread()` + `spawn_watchdog()`，
/// 确保两层防线同时就位。
pub fn spawn_all_monitors() {
    spawn_monitor_thread();
    spawn_watchdog();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_monitor_defaults() {
        let monitor = MemoryMonitor::new();
        assert!(monitor.total_physical() > 0);
        assert!(monitor.soft_limit() > 0);
        assert_eq!(
            monitor.soft_limit(),
            monitor
                .total_physical()
                .saturating_sub(DEFAULT_RESERVE_BYTES)
        );
        // 初始失败计数为 0
        assert_eq!(
            monitor
                .rss_fail_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn test_current_rss_returns_nonzero() {
        let rss = get_current_rss();
        if cfg!(target_os = "linux") {
            assert!(rss > 0, "Linux 下 RSS 应该 > 0");
        }
    }

    #[test]
    fn test_usage_ratio() {
        let monitor = MemoryMonitor::new();
        let ratio = monitor.usage_ratio();
        if cfg!(target_os = "linux") {
            assert!(ratio > 0.0 && ratio < 1.0);
        }
    }

    #[test]
    fn test_global_is_singleton() {
        let a = MemoryMonitor::global() as *const MemoryMonitor;
        let b = MemoryMonitor::global() as *const MemoryMonitor;
        assert_eq!(a, b, "global() 应该返回相同实例");
    }

    #[test]
    fn test_check_inner_normal() {
        // 正常运行时 should not return Some(true)
        let monitor = MemoryMonitor::new();
        let result = monitor.check_inner("test: ");
        // 在 Linux 上应该能读到 RSS，返回 Some(false)
        if cfg!(target_os = "linux") {
            assert_eq!(result, Some(false), "正常状态不应超限");
        }
    }
}
