//! 合成器应用状态与后端代际判定（从 `layout.rs` 拆分，保持文件 < 400 行）。

use lumino_midi_model::multi_port::PortSoundfontMap;

/// 合成器已应用状态：端口布局 + 端口音色库映射。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SynthAppliedState {
    /// 已生效的最大 MIDI 端口。
    pub(super) max_port: u8,
    /// 已生效的端口音色库映射。
    pub(super) port_soundfonts: PortSoundfontMap,
}

/// worker 回传的落定结果。
#[derive(Debug)]
pub(crate) struct SynthApplyOutcome {
    /// worker 成功提交的状态（布局失败时不会产生本值）。
    pub(super) applied: SynthAppliedState,
    /// 音色库部分回退/后端不支持时的可见提示（None = 完全成功）。
    pub(super) notice: Option<String>,
}

/// 合成应用结果是否属于旧后端代际（必须丢弃）。
pub(crate) fn synth_outcome_is_stale(done_epoch: u64, current_epoch: u64) -> bool {
    done_epoch != current_epoch
}
