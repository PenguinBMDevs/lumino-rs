//! 平台专属内存信息获取函数（跨平台）
//!
//! 支持 Linux、macOS、Windows 三平台，分别使用对应平台 API：
//! - **Linux**:   `/proc/meminfo`（总内存）、`/proc/self/status`（RSS）
//! - **macOS**:  `sysctl HW_MEMSIZE`（总内存）、`task_info`（RSS）
//! - **Windows**: `GlobalMemoryStatusEx`（总内存）、`GetProcessMemoryInfo`（RSS）
//!
//! 提取为独立子模块，供 MemoryMonitor 和看门狗共用。

/// 检测失败时的兜底总内存（8 GB）
const FALLBACK_TOTAL_MEMORY: u64 = 8 * 1024 * 1024 * 1024;

// =============================================================================
// 获取总物理内存
// =============================================================================

#[cfg(target_os = "linux")]
pub fn get_total_physical_memory() -> u64 {
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
                return kb * 1024;
            }
        }
    }

    tracing::warn!("MemoryMonitor: /proc/meminfo 中未找到 MemTotal, 使用兜底值");
    FALLBACK_TOTAL_MEMORY
}

#[cfg(target_os = "macos")]
pub fn get_total_physical_memory() -> u64 {
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
pub fn get_total_physical_memory() -> u64 {
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
// 获取当前 RSS
// =============================================================================

#[cfg(target_os = "linux")]
pub fn get_current_rss() -> u64 {
    let content = match std::fs::read_to_string("/proc/self/status") {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("MemoryMonitor: 无法读取 /proc/self/status ({e}), 跳过检查");
            return 0;
        }
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

    tracing::warn!("MemoryMonitor: /proc/self/status 中未找到 VmRSS");
    0
}

#[cfg(target_os = "macos")]
pub fn get_current_rss() -> u64 {
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
pub fn get_current_rss() -> u64 {
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
// 获取进程 CPU 时间（微秒）
// =============================================================================

/// 获取当前进程已消耗的 CPU 时间总和（用户态 + 内核态，单位微秒）。
///
/// 用于计算 CPU 使用率：(delta_cpu / delta_wall) * 100%
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn get_process_cpu_time_us() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if ret == 0 {
        let usage = unsafe { usage.assume_init() };
        let user_us =
            usage.ru_utime.tv_sec as u64 * 1_000_000 + usage.ru_utime.tv_usec as u64;
        let sys_us =
            usage.ru_stime.tv_sec as u64 * 1_000_000 + usage.ru_stime.tv_usec as u64;
        user_us + sys_us
    } else {
        0
    }
}

/// 获取当前进程已消耗的 CPU 时间总和（用户态 + 内核态，单位微秒）。
#[cfg(target_os = "windows")]
pub fn get_process_cpu_time_us() -> u64 {
    #[repr(C)]
    struct FileTime {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            h_process: *mut std::ffi::c_void,
            lp_creation_time: *mut FileTime,
            lp_exit_time: *mut FileTime,
            lp_kernel_time: *mut FileTime,
            lp_user_time: *mut FileTime,
        ) -> i32;
    }

    unsafe {
        let mut creation = FileTime {
            dw_low_date_time: 0,
            dw_high_date_time: 0,
        };
        let mut exit = FileTime {
            dw_low_date_time: 0,
            dw_high_date_time: 0,
        };
        let mut kernel = FileTime {
            dw_low_date_time: 0,
            dw_high_date_time: 0,
        };
        let mut user = FileTime {
            dw_low_date_time: 0,
            dw_high_date_time: 0,
        };

        if GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        ) != 0
        {
            let kernel_100ns =
                ((kernel.dw_high_date_time as u64) << 32) | kernel.dw_low_date_time as u64;
            let user_100ns =
                ((user.dw_high_date_time as u64) << 32) | user.dw_low_date_time as u64;
            // FILETIME units are 100-nanosecond intervals
            (kernel_100ns + user_100ns) / 10
        } else {
            0
        }
    }
}
