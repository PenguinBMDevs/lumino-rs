//! **DRI（单人负责）**: lumino-core 内存诊断模块负责人。所有 `unsafe`/`SIGKILL` 逻辑变更须经其 review。
//!
//! 看门狗（Watchdog）— 完全独立的终极防线
//!
//! 架构独立性：
//! - 启动时捕获主进程 PID，用 PID 构造路径（Linux: /proc/{pid}/status, Windows: GetProcessMemoryInfo）
//! - 自己的内存读取函数（watchdog_get_*），不调用 MemoryMonitor 的任何方法
//! - 自己的阈值逻辑（RSS > 100% soft_limit OR 系统可用 < 350MB）
//! - 自己的终止手段（SIGKILL / TerminateProcess — 不可阻塞/忽略）
//!
//! 时序：
//! - 主监控在 95% soft_limit abort，触发时有 backtrace
//! - 看门狗在 100% soft_limit 或系统内存 ≤ 350MB 时 SIGKILL
//! - 50ms 轮询确保在突发分配瞬间吞掉最后 350MB 之前拦截
//!
//! 生效范围（Lumino 定制）：**只监控 MIDI 加载期间的内存**。
//! - 加载路径（`runner::menu::file::load` 等）在加载前确保看门狗已启动，
//!   加载开始前置位 `midi_guard::set_midi_load_active(true)`；
//! - 看门狗每轮检查该标志：非加载状态一律不检查、不终止；
//! - 加载结束（成功或失败）清除标志，看门狗恢复休眠。
//! - 除加载 MIDI 之外的场景一律不监控。

/// 以下三项只在非 macOS 平台使用：macOS 上看门狗被禁用（见 [`spawn_watchdog()`]），
/// 编译该平台的实现会触发 dead-code 警告。
#[cfg(not(target_os = "macos"))]
use std::sync::OnceLock;

/// 看门狗检查间隔（毫秒）— 50ms，只读 /proc 一行，零 CPU 开销
#[cfg(not(target_os = "macos"))]
const WATCHDOG_INTERVAL_MS: u64 = 50;

/// 看门狗系统可用内存阈值：低于此值时直接 SIGKILL
#[cfg(not(target_os = "macos"))]
const WATCHDOG_MIN_AVAILABLE_BYTES: u64 = 350 * 1024 * 1024; // 350mb 应该够

// ── 平台相关：通过 PID 获取进程 RSS（完全独立，不依赖 /proc/self）──

#[cfg(target_os = "linux")]
fn watchdog_get_process_rss(pid: u32) -> u64 {
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
        let pmc_result = winapi::um::psapi::GetProcessMemoryInfo(handle, &mut pmc, pmc.cb);
        winapi::um::handleapi::CloseHandle(handle);
        if pmc_result != 0 {
            pmc.WorkingSetSize as u64
        } else {
            0
        }
    }
}

// ── 平台相关：获取系统可用内存 ──

#[cfg(target_os = "linux")]
fn watchdog_get_available_memory() -> u64 {
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

// macOS 也是 unix，但 macOS 上整个看门狗被禁用（无调用者），需显式排除以免 dead-code
#[cfg(all(unix, not(target_os = "macos")))]
fn watchdog_force_kill(pid: u32) {
    let kill_result = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    if kill_result != 0 {
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
/// - 自己的 PID 引用（启动时捕获，跨平台读取进程 RSS）
/// - 自己的内存读取函数（watchdog_get_*），零依赖 MemoryMonitor
/// - 自己的阈值（soft_limit 100%，主监控在 95% 已动作，看门狗兜底）
/// - 系统可用内存阈值（< 350MB 时即使 RSS 未超限也触发，macOS 使用 host_statistics64）
/// - 自己的终止手段（SIGKILL / TerminateProcess，不可被捕获/阻塞/忽略）
///
/// 返回 `true` 表示线程已启动（或之前已启动），`false` 表示创建线程失败。
///
/// ## 平台说明
/// - **Linux / Windows**: 正常启动看门狗线程
/// - **macOS**: 禁用，第一个没法用，第二个不需要，系统自己有（见 [`super::spawn_all_monitors()`] 说明）
#[cfg(not(target_os = "macos"))]
pub fn spawn_watchdog() -> bool {
    static SPAWNED: OnceLock<std::thread::JoinHandle<()>> = OnceLock::new();

    if SPAWNED.get().is_some() {
        return true;
    }

    let pid = std::process::id();
    let total = crate::memory_monitor::platform::get_total_physical_memory();
    let soft_limit = total.saturating_sub(crate::memory_monitor::DEFAULT_RESERVE_BYTES);

    tracing::info!(
        "MemoryWatchdog: 启动 (PID={}, soft_limit={} MB, sys_available_min={} MB, poll={}ms)",
        pid,
        soft_limit / 1024 / 1024,
        WATCHDOG_MIN_AVAILABLE_BYTES / 1024 / 1024,
        WATCHDOG_INTERVAL_MS,
    );

    match std::thread::Builder::new()
        .name("memory-watchdog".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(WATCHDOG_INTERVAL_MS));

                // 只监控 MIDI 加载期间的内存：非加载状态一律不检查、不终止。
                // 看门狗在加载路径（load_midi_file 等）加载前确保已启动，
                // 加载开始前置位 MIDI_LOAD_ACTIVE，加载结束（成功/失败）后清除。
                if !crate::memory_monitor::midi_guard::midi_load_active() {
                    continue;
                }

                let rss = watchdog_get_process_rss(pid);
                let available = watchdog_get_available_memory();

                let rss_over_limit = rss > 0 && rss > soft_limit;
                let system_critical = available < WATCHDOG_MIN_AVAILABLE_BYTES;

                if rss_over_limit || system_critical {
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

                    watchdog_force_kill(pid);
                    std::process::abort();
                }
            }
        }) {
        Ok(handle) => {
            let _ = SPAWNED.set(handle);
            tracing::info!(
                "MemoryWatchdog: 看门狗线程已启动 (PID={}, interval={}ms, sys_available_min={}MB)",
                pid,
                WATCHDOG_INTERVAL_MS,
                WATCHDOG_MIN_AVAILABLE_BYTES / 1024 / 1024,
            );
            true
        }
        Err(e) => {
            tracing::error!("MemoryWatchdog: 无法创建看门狗线程: {e}");
            false
        }
    }
}

/// macOS 上禁用看门狗
///
/// TODO: macOS 内存监控 — 当前 macOS 上禁用了看门狗。
/// 后续方案：使用 `dispatch_source` 监听 macOS 的 `memorypressure` 事件，
/// 仅在系统内存压力升高时触发检查，而非固定间隔轮询。
#[cfg(target_os = "macos")]
pub fn spawn_watchdog() -> bool {
    true
}
