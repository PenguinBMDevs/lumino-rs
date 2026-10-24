//! 加载期防护开关（设置面板「加载保护」）
//!
//! 开关真源在用户配置（`UiConfig::load_memory_check_enabled` /
//! `UiConfig::load_watchdog_enabled`，默认均为**关闭**）。Runner 在每次加载
//! 开始前把配置同步到本模块的进程级原子标志；加载链路深处（midi-loader /
//! midi-model / 后台监控线程 / 看门狗线程）读标志决定是否检查、是否终止进程。
//!
//! 这样做的原因：解析链路位于 `lumino-midi-loader` / `lumino-midi-model`，
//! 让它们依赖配置类型会把存储概念下推到数据层；原子标志是进程内唯一真源，
//! 既零传参成本，也不引入 crate 依赖。
//!
//! 语义：
//! - [`load_memory_check_enabled`]：加载期间是否执行内存检查——同步检查
//!   （[`super::MemoryMonitor::check_loading()`]）与后台监控在加载期间的检查。
//!   关闭时加载期间不执行阈值判定、不因软限终止（进度显示仍会读取 RSS）。
//! - [`load_watchdog_enabled`]：加载期间是否启用进程强杀看门狗
//!   （SIGKILL / TerminateProcess）。
//!
//! 标志可随时切换：后台监控与看门狗线程每轮读取，加载中途关闭即停止检查；
//! 同步检查（`check_loading`）在每次调用点读取当前值。因此设置面板改完开关
//! 立即生效，不必等下一次加载。

use std::sync::atomic::{AtomicBool, Ordering};

/// 加载期间内存检查开关（默认关闭）
static LOAD_MEMORY_CHECK_ENABLED: AtomicBool = AtomicBool::new(false);

/// 加载期间进程强杀看门狗开关（默认关闭）
static LOAD_WATCHDOG_ENABLED: AtomicBool = AtomicBool::new(false);

/// 设置「加载期间内存检查」开关。
pub fn set_load_memory_check_enabled(enabled: bool) {
    LOAD_MEMORY_CHECK_ENABLED.store(enabled, Ordering::SeqCst);
}

/// 「加载期间内存检查」是否开启（默认关闭）。
pub fn load_memory_check_enabled() -> bool {
    LOAD_MEMORY_CHECK_ENABLED.load(Ordering::SeqCst)
}

/// 设置「加载期间进程强杀看门狗」开关。
pub fn set_load_watchdog_enabled(enabled: bool) {
    LOAD_WATCHDOG_ENABLED.store(enabled, Ordering::SeqCst);
}

/// 「加载期间进程强杀看门狗」是否开启（默认关闭）。
pub fn load_watchdog_enabled() -> bool {
    LOAD_WATCHDOG_ENABLED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认关闭 + 开关可往返。
    ///
    /// 本 crate 内只有本测试读写这两个全局标志，串行断言不会与其它用例互扰；
    /// 结束前复位，避免污染同进程的后续测试。
    #[test]
    fn test_load_guard_defaults_off_and_toggles() {
        // 先显式复位，避免将来出现并行用例时依赖"初始值恰好是 false"。
        set_load_memory_check_enabled(false);
        set_load_watchdog_enabled(false);
        assert!(
            !load_memory_check_enabled(),
            "加载期间内存检查必须默认关闭（不在加载时主动终止进程）"
        );
        assert!(
            !load_watchdog_enabled(),
            "进程强杀看门狗必须默认关闭（不在加载时 SIGKILL）"
        );

        set_load_memory_check_enabled(true);
        set_load_watchdog_enabled(true);
        assert!(load_memory_check_enabled());
        assert!(load_watchdog_enabled());

        set_load_memory_check_enabled(false);
        set_load_watchdog_enabled(false);
        assert!(!load_memory_check_enabled());
        assert!(!load_watchdog_enabled());
    }
}
