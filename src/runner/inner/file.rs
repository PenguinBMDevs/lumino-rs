//! 文件操作状态管理
//!
//! RunnerInner 中与文件操作相关的实现。
//! 当前文件操作方法主要在 menu/file/ 系列模块中。

use lumino_core::storage::config::UiConfig;
use lumino_diagnostics::memory_monitor::{load_guard, midi_guard, watchdog};

/// 从配置读取「加载保护」两个开关（字段一一对应，单测锁死映射）。
fn load_guard_flags(ui: &UiConfig) -> (bool, bool) {
    (ui.load_memory_check_enabled, ui.load_watchdog_enabled)
}

impl super::RunnerInner {
    /// 把「加载保护」开关从用户配置同步到诊断模块的进程级标志。
    ///
    /// 设置面板只写 `UiConfig`；加载链路深处（midi-loader / midi-model /
    /// 后台监控线程 / 看门狗线程）读的是 [`load_guard`] 的原子标志，
    /// 这样数据层不必依赖配置类型。每次加载开始前调用一次，保证使用最新
    /// 配置；设置保存时也会调用一次，让开关即时落到运行时（加载中途关闭
    /// 看门狗会立刻停止检查）。
    pub(crate) fn sync_load_guard(&self) {
        let ui = &self.window_state.storage.config.get().ui;
        let (memory_check, watchdog) = load_guard_flags(ui);
        load_guard::set_load_memory_check_enabled(memory_check);
        load_guard::set_load_watchdog_enabled(watchdog);
        tracing::debug!(
            "加载保护开关: 内存检查={}, 看门狗={}（默认均为关闭）",
            memory_check,
            watchdog
        );
    }

    /// 开始一次 MIDI / 工程加载：刷新开关、按需启动看门狗、置位加载状态。
    ///
    /// 返回的 [`midi_guard::MidiLoadGuard`] 必须由加载任务持有到结束——
    /// 它保证任何退出路径（正常 / 提前返回 / 取消 / panic 展开）都会清除
    /// 加载标志，不会让看门狗与后台监控误判为"仍在加载"。
    pub(crate) fn begin_midi_load(&self) -> midi_guard::MidiLoadGuard {
        self.sync_load_guard();
        watchdog::spawn_watchdog();
        midi_guard::MidiLoadGuard::activate()
    }
}

#[cfg(test)]
mod tests {
    use super::load_guard_flags;
    use lumino_core::storage::config::UiConfig;

    /// 配置字段 → 运行时开关的映射不得串位（两个 bool 相邻，写反不报错）。
    #[test]
    fn test_load_guard_flags_mapping() {
        let ui = UiConfig {
            load_memory_check_enabled: true,
            load_watchdog_enabled: false,
            ..UiConfig::default()
        };
        assert_eq!(
            load_guard_flags(&ui),
            (true, false),
            "内存检查开关不得映射到看门狗"
        );

        let ui = UiConfig {
            load_memory_check_enabled: false,
            load_watchdog_enabled: true,
            ..UiConfig::default()
        };
        assert_eq!(
            load_guard_flags(&ui),
            (false, true),
            "看门狗开关不得映射到内存检查"
        );
    }
}
