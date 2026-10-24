//! MIDI 加载状态标志（看门狗门控）
//!
//! 看门狗**只在 MIDI 加载期间**监控内存：加载路径（`load.rs` / `lifecycle.rs` /
//! `export.rs`）在加载开始前调用 [`MidiLoadGuard::activate`]（或
//! `set_midi_load_active(true)`）标记加载状态，加载结束（成功或失败）后清除。
//!
//! 看门狗线程每轮检查该标志：非加载状态一律不检查内存、不触发终止，
//! 即"除了加载 MIDI 之外的场景一律不监控"。
//!
//! **优先使用 RAII 守卫**：异步加载任务的退出路径不止"成功/失败"两种
//! （提前 return、任务取消、panic 展开），手工 `set_midi_load_active(false)`
//! 一旦漏掉，标志会卡在 true——看门狗与后台监控会误判为"仍在加载"。

use std::sync::atomic::{AtomicBool, Ordering};

/// MIDI 加载进行中（看门狗仅在置位期间监控内存）
static MIDI_LOAD_ACTIVE: AtomicBool = AtomicBool::new(false);

/// 标记 MIDI 加载状态（加载开始前置位，加载结束后清除）
pub fn set_midi_load_active(active: bool) {
    MIDI_LOAD_ACTIVE.store(active, Ordering::SeqCst);
}

/// 当前是否处于 MIDI 加载状态（看门狗门控）
pub fn midi_load_active() -> bool {
    MIDI_LOAD_ACTIVE.load(Ordering::SeqCst)
}

/// MIDI 加载 RAII 守卫：创建时置位加载状态，`Drop` 时清除。
///
/// 加载任务应持有它直到任务结束——正常完成、提前 return、取消或 panic
/// 展开都会清标志，避免 `MIDI_LOAD_ACTIVE` 卡在 true 导致看门狗/后台监控
/// 在非加载期间长期误判。
pub struct MidiLoadGuard(());

impl MidiLoadGuard {
    /// 置位加载状态并返回守卫。
    pub fn activate() -> Self {
        set_midi_load_active(true);
        Self(())
    }
}

impl Drop for MidiLoadGuard {
    fn drop(&mut self) {
        set_midi_load_active(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 守卫必须置位并在 Drop 时清除（任何退出路径都安全）。
    #[test]
    fn test_guard_activates_and_clears_on_drop() {
        assert!(!midi_load_active(), "默认不在加载中");
        {
            let _guard = MidiLoadGuard::activate();
            assert!(midi_load_active(), "守卫存活期间必须保持加载状态");
        }
        assert!(
            !midi_load_active(),
            "守卫 Drop 必须清除加载标志（提前返回 / 取消 / panic 展开路径）"
        );
    }
}
