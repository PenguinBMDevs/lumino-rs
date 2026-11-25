//! REND-005：端口音色库期望值的提交与播放态延迟应用。
//!
//! 工程态（UI/持久化）只提交期望值；真正的 SF2 解析与事件下发由后台
//! worker 完成（见 `layout.rs`）。播放/暂停期间提交的映射会保留到停播后，
//! 避免 NoteOff release 使用新矩阵导致释放尾音音色错误。

use lumino_core::storage::config::SynthBackend;
use lumino_core::tr;
use lumino_midi_model::multi_port::PortSoundfontMap;
use std::sync::atomic::Ordering;

use super::layout::{LayoutDecision, decide_layout_request};
use super::*;

impl MidiManager {
    /// 提交工程态的“端口 → 音色库”映射（REND-005）。
    ///
    /// - 空闲：立即排程后台应用（不重建管线、不杀音）；
    /// - 播放/暂停：只提交期望值，停播后由 [`Self::tick_port_soundfonts`] 应用，
    ///   并给出一次可见提示；
    /// - 后端不支持（System/Kdmapi）：给出可见提示，不反复排程。
    ///
    /// TODO(REND-005 P3/P4)：工程持久化与工程设置窗口接线后由生产路径调用；
    /// 当前仅由状态机单测覆盖，接线时移除 `#[allow(dead_code)]`。
    #[allow(dead_code)]
    pub fn apply_port_soundfonts_deferred(&mut self, map: PortSoundfontMap) {
        if self.desired_port_soundfonts == map {
            return;
        }
        self.desired_port_soundfonts = map.clone();
        *self
            .port_soundfonts_desired
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = map.clone();

        let software_backend = matches!(
            self.active_backend,
            SynthBackend::XSynth | SynthBackend::Lgs | SynthBackend::Maestro
        );
        if !software_backend && !self.software_backend_pending() && !map.is_empty() {
            self.pending_backend_notice = Some(
                tr!("当前后端不支持按端口音色库，映射仅在软件合成后端（XSynth）生效").to_string(),
            );
            tracing::info!("MIDI: 当前后端不支持端口音色库，映射已暂存");
            return;
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
        if playback_active || self.desired_port_soundfonts == self.spawned_port_soundfonts {
            return;
        }
        self.maybe_start_synth_apply();
    }

    /// 按当前后端/在途状态决定是否启动后台应用（合并到既有 worker）。
    fn maybe_start_synth_apply(&mut self) {
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
