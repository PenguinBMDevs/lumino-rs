use super::*;

/// The result of an offline render.
#[derive(Debug, Clone)]
pub struct RenderResult {
    /// Interleaved samples (L/R/L/R...) for the whole render.
    pub samples: Vec<f32>,
    /// Output sample rate.
    pub sample_rate: u32,
    /// Number of channels (1 or 2).
    pub channels: u32,
    /// Total rendered frames.
    pub frames: u64,
}

/// Hard ceiling for a single offline render (≈ 13.6 h @ 64 kHz). Keeps the
/// guard well below the 2^32 frame range of the u32 GPU timestamps.
pub(crate) const MAX_RENDER_FRAMES: u64 = 1 << 31;

/// Cooperative checkpoint for offline render loops: called once per rendered
/// block (and once per prewarm chunk); returning `false` cancels the render
/// with [`SynthError::Cancelled`]. Implementations may block inside the
/// closure to implement pause (e.g. the export layer waits on its
/// `AudioExportControl`).
pub type RenderCheckpoint = Arc<dyn Fn() -> bool + Send + Sync>;

/// 离线渲染的进度事件（供导出 UI 展示真实进度；不参与音频数据流）。
///
/// `done` / `total` 的语义随阶段不同：
/// - [`RenderProgress::Prewarm`]：音色重采样/上传的采样条目数；
/// - [`RenderProgress::Render`]：已渲染帧数 / 渲染地平线帧数
///   （`events_end + max_tail`，被限帧调用时以调用方给出的帧数为准）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderProgress {
    /// 预载音色库：重采样并上传离线渲染需要的采样。
    Prewarm {
        /// 已处理的采样条目数。
        done: u64,
        /// 需要处理的总采样条目数。
        total: u64,
    },
    /// 主渲染：按块推进。
    Render {
        /// 已渲染帧数（不超过 `total`）。
        done: u64,
        /// 渲染地平线总帧数。
        total: u64,
    },
}

/// 离线渲染进度回调：在渲染线程内按预载分片 / 渲染块调用。
///
/// 回调必须轻量且不可阻塞（只做转发），节流由调用方负责（见 `lumino-export`
/// 的 `gpu_backend`）。回调为 `Fn`，可以被安全地重复调用。
pub type RenderProgressFn = Arc<dyn Fn(RenderProgress) + Send + Sync>;

/// 不依赖 `&self` 的进度转发（供 `&mut self` 字段被借用的热循环使用）。
pub(crate) fn report_progress(callback: &Option<RenderProgressFn>, progress: RenderProgress) {
    if let Some(cb) = callback {
        cb(progress);
    }
}

/// Runs a checkpoint without needing `&self` (lets hot loops keep a cloned
/// handle while other `self` fields are mutably borrowed).
pub(crate) fn checkpoint_ok(checkpoint: &Option<RenderCheckpoint>) -> Result<(), SynthError> {
    match checkpoint {
        Some(cb) if !cb() => Err(SynthError::Cancelled),
        _ => Ok(()),
    }
}

/// Voice states are read back every N-th block (see
/// `GpuSynth::states_sync_counter`).
///
/// This must be small: ended voices are only pruned after a readback, and
/// with a large block size a lag of a few blocks lets thousands of dead
/// voices accumulate (dense MIDI adds thousands per block), bloating the
/// GPU voice pool to the physical buffer-size wall. One block of lag is the
/// right trade-off (a single extra map per block is cheap). Voice states are
/// read back every `STATES_SYNC_EVERY` blocks: a voice ending late does not
/// change any audio sample, and skipping the extra map/poll round trip per
/// block is a large CPU win. Kept at 1 for exactness - raising it resumes
/// voices from stale states (risk of audio replay), which the user forbade.
pub(crate) const STATES_SYNC_EVERY: u32 = 1;

/// Headroom between the polyphony trim target and the physical GPU pool:
/// voices trimmed for polyphony fade out over 1 ms (one block), so the
/// pool must be able to hold the active voices PLUS one block's worth of
/// fading voices. `max_voices` stays the *active* polyphony target (and the
/// trim threshold); the pool is sized 1.5x so a sudden black-MIDI storm
/// fades everything out smoothly instead of hard-killing at the cap.
/// The fading voices are pruned by the next readback (they end after the
/// 1 ms fade), so the surplus is transient and the pool returns to
/// `max_voices` when the storm passes.
pub(crate) const FADE_SLOTS_FRACTION: usize = 2; // pool = max_voices * (1 + 1/FRACTION)

/// Hard cap for the voice output buffer (per-voice output for one block).
/// The wgpu/D3D12-style maximum buffer size is 2 GiB - 1; staying well
/// below it keeps headroom. `max_voices` must be chosen so the *peak*
/// active voice count stays under this (a dense MIDI may need 32k+).
/// For unlimited mode (max_voices == 0) the cap is effectively the device
/// maximum minus a small guard so black MIDI can use up to ~500k voices
/// per block at 512 frames (524k = (2 GiB - 64 KiB) / (512*8)).
pub(crate) const MAX_VOICE_OUT_BYTES: u64 = (1 << 31) - (1 << 16); // ~2 GiB - 64 KiB

/// A submission whose readback is still outstanding (see `GpuSynth::pending`).
pub(crate) struct PendingReadback {
    pub(crate) idx: wgpu::SubmissionIndex,
    /// `out_readback` slot the block's audio was copied to.
    pub(crate) out_slot: usize,
    /// `states_readback` slot the block's voice states were copied to.
    pub(crate) states_slot: usize,
}

/// Cache key for one note's voice templates (see `GpuSynth::voice_templates`).
pub(crate) type VoiceTemplateKey = (u8, u8, u8, u32, u8, u8);
/// Pre-built voices per note, keyed by (key, vel, channel, pitch_mult_bits,
/// env_attack, env_release).
pub(crate) type VoiceTemplateCache = std::collections::HashMap<VoiceTemplateKey, Vec<Voice>>;

/// Per-voice diagnostic row from [`GpuSynth::debug_voices`].
pub(crate) type VoiceDebugInfo = (u8, u8, f32, f32, bool, bool, u32, u32, u64, u32, f32);

/// 每键每块 note-on 硬上限（纯函数便于单测）。
///
/// 这是**防病态风暴**的保护，不是音乐意义的每键复音上限；后者由
/// `trim_key_voices` 在 spawn 之后保证（XSynth 语义：新音必发声，抢最安静
/// 老组）。旧实现把 `max_voices_per_key` 当预算，丢的是**最新**音符，
/// 密集段落大量缺音（实测黑 MIDI 在 limit=4 下丢 18% note-on）。
pub(crate) const MAX_SPAWNS_PER_KEY_PER_BLOCK: u32 = 65_536;

#[inline]
pub(crate) fn spawn_budget_allows(used: u32) -> bool {
    used < MAX_SPAWNS_PER_KEY_PER_BLOCK
}
