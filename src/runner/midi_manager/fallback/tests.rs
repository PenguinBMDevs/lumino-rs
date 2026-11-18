//! `fallback` 单元测试：降级目标、会话覆盖、冷却与一次性约束。
//!
//! 只测状态机与纯函数；不触发真实 XSynth/System/WinMM 初始化。

use std::time::{Duration, Instant};

use lumino_core::storage::config::SynthBackend;

use super::*;
use crate::runner::midi_manager::MidiManager;

fn lgs_manager() -> MidiManager {
    MidiManager {
        active_backend: SynthBackend::Lgs,
        ..MidiManager::default()
    }
}

/// 有音色库优先 XSynth；没有则 System（System 是最后兜底）。
#[test]
fn decide_degrade_target_covers_both_targets() {
    assert_eq!(decide_degrade_target(true), SynthBackend::XSynth);
    assert_eq!(decide_degrade_target(false), SynthBackend::System);
}

/// 无覆盖时有效后端就是用户偏好。
#[test]
fn effective_backend_follows_preference_without_override() {
    let manager = MidiManager::default();
    assert_eq!(
        manager.effective_backend(SynthBackend::Lgs),
        SynthBackend::Lgs
    );
}

/// 降级置覆盖 + needs_reinit + 可见提示，并立即影响有效后端。
#[test]
fn degrade_sets_override_and_requests_reinit() {
    let mut manager = lgs_manager();
    manager.xsynth_soundfont_path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml").to_string();

    assert!(manager.degrade_from_lgs_exhaustion());

    assert_eq!(manager.backend_override(), Some(SynthBackend::XSynth));
    assert_eq!(
        manager.effective_backend(SynthBackend::Lgs),
        SynthBackend::XSynth
    );
    assert!(manager.needs_reinit(), "降级必须交由同帧重初始化执行");
    let notice = manager.take_pending_backend_notice().expect("应提示降级");
    assert!(notice.contains("XSynth"), "提示应说明目标后端: {notice}");
    assert!(
        notice.contains("LGS"),
        "提示应说明用户偏好仍为 LGS: {notice}"
    );
}

/// 没有可用音色库时降级到 System，而不是继续尝试 XSynth。
#[test]
fn degrade_falls_back_to_system_without_soundfont() {
    let mut manager = lgs_manager();

    assert!(manager.degrade_from_lgs_exhaustion());

    assert_eq!(manager.backend_override(), Some(SynthBackend::System));
    let notice = manager.take_pending_backend_notice().expect("应提示降级");
    assert!(notice.contains("MIDI"), "提示应说明目标后端: {notice}");
}

/// 同一轮故障只自动降级一次；用户显式重置后才允许再次降级。
#[test]
fn auto_degrade_is_one_shot_until_cleared() {
    let mut manager = lgs_manager();
    assert!(manager.degrade_from_lgs_exhaustion());
    manager.take_pending_backend_notice();

    assert!(
        !manager.degrade_from_lgs_exhaustion(),
        "同一轮故障不得重复自动降级"
    );

    manager.clear_backend_override();
    assert_eq!(manager.backend_override(), None);
    assert!(
        !manager.backend_fallback.auto_degrade_used,
        "显式重置后应允许再次降级"
    );

    // 模拟交给重初始化后回到 LGS：清掉待处理标志，允许下一轮降级。
    manager.needs_reinit = false;
    assert!(manager.degrade_from_lgs_exhaustion());
}

/// 冷却期内设备事件不得重试 GPU；冷却期后清除覆盖并安排整后端重初始化。
#[test]
fn retry_requires_cooldown() {
    let mut manager = lgs_manager();
    manager.backend_fallback.override_backend = Some(SynthBackend::System);
    manager.backend_fallback.auto_degrade_used = true;
    manager.backend_fallback.degraded_at = Some(Instant::now());

    assert!(
        !manager.retry_lgs_after_degrade(),
        "冷却期内不得重新启用 LGS"
    );
    assert!(!manager.needs_reinit());

    manager.backend_fallback.degraded_at =
        Some(Instant::now() - GPU_RETRY_COOLDOWN - Duration::from_secs(1));
    assert!(manager.retry_lgs_after_degrade());

    assert_eq!(manager.backend_override(), None);
    assert!(manager.needs_reinit());
    let notice = manager.take_pending_backend_notice().expect("应提示恢复");
    assert!(notice.contains("LGS"), "{notice}");
}

/// 非 LGS 活跃后端（例如已经切到 XSynth）不得再触发 LGS 降级。
#[test]
fn degrade_ignores_non_lgs_backend() {
    let mut manager = MidiManager {
        active_backend: SynthBackend::XSynth,
        ..MidiManager::default()
    };
    assert!(!manager.degrade_from_lgs_exhaustion());
    assert!(!manager.needs_reinit());
    assert_eq!(manager.backend_override(), None);
}
