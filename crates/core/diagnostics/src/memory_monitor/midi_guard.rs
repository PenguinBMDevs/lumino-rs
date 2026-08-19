//! MIDI 加载状态标志（看门狗门控）
//!
//! 看门狗**只在 MIDI 加载期间**监控内存：加载路径（`load.rs` / `lifecycle.rs` /
//! `export.rs`）在加载开始前调用 `set_midi_load_active(true)` 标记加载状态，
//! 加载结束（成功或失败）后调用 `set_midi_load_active(false)` 清除。
//!
//! 看门狗线程每轮检查该标志：非加载状态一律不检查内存、不触发终止，
//! 即"除了加载 MIDI 之外的场景一律不监控"。

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
