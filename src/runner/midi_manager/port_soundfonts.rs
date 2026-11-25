//! REND-005：端口音色库期望值的提交、后端能力判定与播放态延迟应用。
//!
//! 工程态（UI/持久化）只提交期望值；真正的 SF2 解析与事件下发由后台
//! worker 完成（见 `layout.rs`）。播放/暂停期间提交的映射会保留到停播后，
//! 避免 NoteOff release 使用新矩阵导致释放尾音音色错误。
//!
//! 后端能力是硬边界：只有 XSynth 实现按端口音色库。LGS/Maestro/System/Kdmapi
//! 属于**降级运行**——映射照常保存，但当前播放/导出使用全局音色库；Runner
//! 会用独立弹窗告知用户，而不是只在状态栏一闪而过。

use lumino_core::storage::config::SynthBackend;
use lumino_core::tr;
use lumino_midi_model::multi_port::PortSoundfontMap;
use std::sync::atomic::Ordering;

use super::layout::{LayoutDecision, decide_layout_request};
use super::*;

/// REND-005：「当前后端不支持按端口音色库」提示数据。
///
/// 映射已保存；当前后端降级使用全局音色库。Runner 据此弹窗，用户可
/// 继续降级运行或打开合成器设置切到 XSynth。
#[derive(Debug, Clone)]
pub struct PortSoundfontsUnsupportedNotice {
    /// 当前实际生效的后端（用于文案）。
    pub backend: SynthBackend,
    /// 已配置的端口数（映射条目数）。
    pub ports: usize,
}

/// 当前/即将生效后端对“按端口音色库”的能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortSoundfontsCapability {
    /// 当前就是 XSynth：可以应用。
    Supported,
    /// 软件后端仍在异步初始化：等落定后再判断/对齐。
    Pending,
    /// 当前后端不支持（LGS/Maestro/System/Kdmapi）：降级运行。
    Unsupported,
}

impl MidiManager {
    /// 提交工程态的“端口 → 音色库”映射（REND-005）。
    ///
    /// - XSynth：空闲立即排程后台应用；播放中暂存到停播；
    /// - 软件后端初始化中：暂存，初始化落定后由对齐逻辑处理；
    /// - 不支持的后端：映射照常保存，记录为降级并请求专用弹窗；
    /// - 空映射：照常提交（用于恢复全局库）。
    pub fn apply_port_soundfonts_deferred(&mut self, map: PortSoundfontMap) {
        if self.desired_port_soundfonts == map {
            return;
        }
        self.desired_port_soundfonts = map.clone();
        *self
            .port_soundfonts_desired
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = map.clone();
        self.port_soundfonts_unsupported_ack = false;

        match self.port_soundfonts_capability() {
            PortSoundfontsCapability::Supported => {}
            PortSoundfontsCapability::Pending => {
                tracing::info!("MIDI: 软件后端初始化中，端口音色库映射暂存待就绪后应用");
                return;
            }
            PortSoundfontsCapability::Unsupported => {
                // 降级：记录为已应用，避免 tick 反复排程；由专用弹窗告知用户。
                self.spawned_port_soundfonts = map;
                self.request_port_soundfonts_unsupported_notice();
                return;
            }
        }

        if self.playback_active.load(Ordering::SeqCst) {
            self.pending_backend_notice =
                Some(tr!("端口音色库映射已保存，将在当前播放结束后生效").to_string());
            tracing::info!("MIDI: 播放中，端口音色库映射暂存待停播后应用");
            return;
        }

        self.maybe_start_synth_apply();
    }

    /// 每帧同步播放态，并在停播后落定待应用的端口音色库映射。
    ///
    /// 由 `RunnerInner::handle_midi_reinit` 调用；播放中只更新标志，不排程。
    pub fn tick_port_soundfonts(&mut self, playback_active: bool) {
        self.playback_active
            .store(playback_active, Ordering::SeqCst);
        self.refresh_port_soundfonts_support();
        if playback_active || self.desired_port_soundfonts == self.spawned_port_soundfonts {
            return;
        }
        self.maybe_start_synth_apply();
    }

    /// 后端能力变化时维护“不支持”弹窗请求。
    ///
    /// - 恢复 XSynth：丢弃尚未展示的旧提示（已展示的由 Runner 在切换成功时关闭）；
    /// - 不支持：把映射记为降级已应用，并请求一次弹窗（每轮映射/后端只请求一次）。
    fn refresh_port_soundfonts_support(&mut self) {
        if self.desired_port_soundfonts.is_empty() {
            return;
        }
        match self.port_soundfonts_capability() {
            PortSoundfontsCapability::Supported => {
                self.pending_port_soundfonts_unsupported = None;
            }
            PortSoundfontsCapability::Pending => {}
            PortSoundfontsCapability::Unsupported => {
                if self.spawned_port_soundfonts != self.desired_port_soundfonts {
                    self.spawned_port_soundfonts = self.desired_port_soundfonts.clone();
                }
                self.request_port_soundfonts_unsupported_notice();
            }
        }
    }

    /// 请求一次“当前后端不支持”弹窗（幂等：本轮已请求则忽略）。
    fn request_port_soundfonts_unsupported_notice(&mut self) {
        if self.desired_port_soundfonts.is_empty() || self.port_soundfonts_unsupported_ack {
            return;
        }
        self.port_soundfonts_unsupported_ack = true;
        self.pending_port_soundfonts_unsupported = Some(PortSoundfontsUnsupportedNotice {
            backend: self.active_backend,
            ports: self.desired_port_soundfonts.len(),
        });
        tracing::info!(
            "MIDI: 当前后端 {:?} 不支持端口音色库，已请求弹窗提示（降级使用全局音色库）",
            self.active_backend
        );
    }

    /// 当前/即将生效后端对按端口音色库的能力。
    fn port_soundfonts_capability(&self) -> PortSoundfontsCapability {
        if self.is_xsynth_initializing()
            || self.is_lgs_initializing()
            || self.is_maestro_initializing
        {
            // 初始化完成前不判断能力：避免把“即将切到 XSynth”误报为不支持。
            return PortSoundfontsCapability::Pending;
        }
        if self.active_backend == SynthBackend::XSynth {
            PortSoundfontsCapability::Supported
        } else {
            PortSoundfontsCapability::Unsupported
        }
    }

    /// 按当前后端/在途状态决定是否启动后台应用（合并到既有 worker）。
    fn maybe_start_synth_apply(&mut self) {
        // 非 XSynth 后端没有 apply 能力，排程只会得到默认 Err；由弹窗路径处理。
        if !matches!(
            self.port_soundfonts_capability(),
            PortSoundfontsCapability::Supported
        ) {
            return;
        }
        match decide_layout_request(
            self.active_backend,
            self.api.is_some(),
            self.layout_apply.is_some(),
        ) {
            LayoutDecision::Start => self.start_layout_apply(),
            // worker 会在防抖结束时读取最新期望值，无需重复排程。
            LayoutDecision::Coalesce => {}
            // 后端未就绪：保留期望值，初始化完成后由对齐逻辑排程。
            LayoutDecision::Stash => {}
        }
    }
}

#[cfg(test)]
mod tests;
