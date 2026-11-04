//! 洋葱皮全局开关 → CPU 光栅帧闸门契约。
//!
//! 用户需求：「关闭就彻底关闭」。CPU 后端的洋葱皮是独立光栅帧，必须与 GPU 实例层
//! 同时停画；但**关闭 ≠ 停用后端**——渲染器、帧缓存、脏区都要留着，重新打开时
//! 才能按累积脏区一次性补齐（不能每帧重光栅化，也不能把脏区丢掉）。

use lumino_core::storage::config::OnionSkinBackend;

use super::{CpuOnionFrameGate, cpu_onion_frame_gate};

/// CPU 后端 + 开关打开 → 正常生成帧。
#[test]
fn cpu_gate_draws_when_backend_cpu_and_switch_on() {
    assert_eq!(
        cpu_onion_frame_gate(OnionSkinBackend::Cpu, true),
        CpuOnionFrameGate::Draw,
        "CPU 后端且开关打开时必须生成/复用 CPU 洋葱皮帧"
    );
}

/// CPU 后端 + 开关关闭 → 不画，但**不得**走停用路径（否则丢帧缓存与脏区）。
#[test]
fn cpu_gate_hides_without_deactivating_when_switch_off() {
    assert_eq!(
        cpu_onion_frame_gate(OnionSkinBackend::Cpu, false),
        CpuOnionFrameGate::Hidden,
        "开关关闭必须只停画（Hidden），不得停用 CPU 后端（Deactivate）"
    );
}

/// GPU 后端（默认）→ 无论开关如何都停用 CPU 洋葱皮，行为与开关前一致。
#[test]
fn cpu_gate_deactivates_when_backend_not_cpu() {
    for onion_visible in [false, true] {
        assert_eq!(
            cpu_onion_frame_gate(OnionSkinBackend::Gpu, onion_visible),
            CpuOnionFrameGate::Deactivate,
            "GPU 后端的 CPU 洋葱皮必须停用（开关={onion_visible}）"
        );
    }
}
