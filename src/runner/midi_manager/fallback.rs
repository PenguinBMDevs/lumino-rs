//! AUD-011（#307）：LGS 重建耗尽后的 CPU 后端降级。
//!
//! 只影响本进程的**有效后端**：`preferred_backend` 仍是用户偏好，自动降级
//! 不写盘、不改配置。用户显式更改合成器设置（`persist::synth_changed`）或
//! 设备恢复事件（冷却期已过）才清除覆盖，回到用户偏好。

use std::path::Path;
use std::time::{Duration, Instant};

use lumino_core::{tr, trf};

use super::*;

/// 降级后再次尝试 LGS 的冷却期（设备事件可能频繁，避免 LGS↔CPU 抖动）。
pub(super) const GPU_RETRY_COOLDOWN: Duration = Duration::from_secs(60);

/// 选择 CPU 降级目标：有可用音色库优先 XSynth，否则 System。
///
/// System 是最后兜底；若 System 也初始化失败，既有 `create_additional_output`
/// 的重试/提示链路会接管——本函数不做设备探测，避免 UI 线程额外 I/O。
#[must_use]
pub(super) fn decide_degrade_target(has_soundfont: bool) -> SynthBackend {
    if has_soundfont {
        SynthBackend::XSynth
    } else {
        SynthBackend::System
    }
}

/// 会话级降级状态（不落盘）。
#[derive(Default)]
pub(super) struct BackendFallbackState {
    /// 覆盖后端；None = 跟随用户偏好
    override_backend: Option<SynthBackend>,
    /// 本轮故障是否已自动降级（用户显式改设置时重置，允许再次尝试）
    auto_degrade_used: bool,
    /// 最近一次降级时刻（冷却期）
    degraded_at: Option<Instant>,
}

impl BackendFallbackState {
    pub(super) fn is_degraded(&self) -> bool {
        self.override_backend.is_some()
    }

    fn cooldown_elapsed(&self) -> bool {
        self.degraded_at
            .is_none_or(|at| at.elapsed() >= GPU_RETRY_COOLDOWN)
    }

    fn clear(&mut self) {
        self.override_backend = None;
        self.auto_degrade_used = false;
        self.degraded_at = None;
    }
}

/// 降级提示里的后端名：避免把 `SynthBackend::Display` 的括号说明塞进句子。
fn cpu_backend_label(target: SynthBackend) -> &'static str {
    match target {
        SynthBackend::XSynth => "XSynth",
        SynthBackend::System => tr!("系统 MIDI"),
        _ => "CPU",
    }
}

fn has_usable_soundfont(path: &str) -> bool {
    !path.is_empty() && Path::new(path).is_file()
}

impl MidiManager {
    /// 当前有效后端：会话覆盖优先，否则用户偏好。
    pub(super) fn effective_backend(&self, preferred: SynthBackend) -> SynthBackend {
        self.backend_fallback.override_backend.unwrap_or(preferred)
    }

    /// LGS 重建耗尽：切换到 CPU 后端，置 `needs_reinit` 由同帧重初始化执行。
    ///
    /// 返回 `false` = 本轮已降级过 / 已有待处理的重初始化 / 当前不是 LGS；
    /// 调用方保留原有「保持静音」提示。
    pub(super) fn degrade_from_lgs_exhaustion(&mut self) -> bool {
        if self.backend_fallback.auto_degrade_used
            || self.needs_reinit
            || self.active_backend != SynthBackend::Lgs
        {
            return false;
        }

        let target = decide_degrade_target(has_usable_soundfont(&self.xsynth_soundfont_path));
        self.backend_fallback.override_backend = Some(target);
        self.backend_fallback.auto_degrade_used = true;
        self.backend_fallback.degraded_at = Some(Instant::now());
        self.needs_reinit = true;
        self.pending_backend_notice = Some(trf!(
            "GPU 合成不可用（重建多次失败），已临时切换到 {backend} 合成；可在设置中重选 LGS 或重启后恢复",
            backend = cpu_backend_label(target)
        ));
        tracing::warn!(
            "LGS（PR-D）: 重建耗尽，临时降级到 {:?}（不写入配置偏好）",
            target
        );
        true
    }

    /// 用户显式更改合成器设置：清除会话覆盖，回到用户偏好。
    pub fn clear_backend_override(&mut self) {
        if self.backend_fallback.is_degraded() {
            tracing::info!("MIDI: 用户更改合成器设置，清除会话级后端覆盖");
        }
        self.backend_fallback.clear();
    }

    /// 降级后收到设备事件：冷却期已过则清除覆盖并安排一次整后端重初始化。
    pub(super) fn retry_lgs_after_degrade(&mut self) -> bool {
        if !self.backend_fallback.is_degraded() || !self.backend_fallback.cooldown_elapsed() {
            return false;
        }
        self.backend_fallback.clear();
        self.needs_reinit = true;
        self.pending_backend_notice =
            Some(tr!("GPU 设备已恢复，正在重新启用 LGS (GPU) 合成…").to_string());
        tracing::info!("LGS（PR-D）: 降级冷却期已过，安排重新启用 LGS 后端");
        true
    }

    /// 测试用：读取当前会话覆盖（生产路径不需要暴露）。
    #[cfg(test)]
    pub(super) fn backend_override(&self) -> Option<SynthBackend> {
        self.backend_fallback.override_backend
    }

    /// 测试用：把管理器的降级状态直接摆到「已降级 N 秒」。
    #[cfg(test)]
    pub(super) fn mark_degraded_for_test(&mut self, target: SynthBackend, elapsed: Duration) {
        self.backend_fallback.override_backend = Some(target);
        self.backend_fallback.auto_degrade_used = true;
        self.backend_fallback.degraded_at = Some(Instant::now() - elapsed);
    }
}

#[cfg(test)]
mod tests;
