//! The `GpuSynth` engine: MIDI event scheduling, voice management and
//! block-wise GPU rendering.

use std::collections::VecDeque;
use std::sync::Arc;

use bytemuck::Zeroable;
use rayon::prelude::*;

use crate::config::{ChannelMode, SynthConfig};
use crate::error::SynthError;
use crate::gpu::{
    EnvStageGpu, GpuResources, GrowableBuffer, MIX_CHANNELS, MixEvent, MixParams, MixStart,
    SAMPLES_CHUNK_BINDING_BASE, SAMPLES_CHUNK_BYTES, SAMPLES_CHUNKS, VoiceParams, VoiceState,
    create_gpu_context,
};
use crate::midi::{MidiEvent, MidiFile, MidiStream, TimedEvent};
use crate::soundfont::SoundFont;
use crate::synth::voices::{Voice, build_voice, refresh_env_stages};

mod bind;
mod cc;
mod channel;
mod dispatch;
mod events;
mod init;
mod limiter;
mod offline;
mod pool;
mod progress;
mod readback;
mod render;
mod streaming;
mod trim;
mod types;
mod upload;
mod voice_alloc;
mod warm;

use limiter::{limit_block, write_samples};
use progress::ProgressBar;
use types::{
    FADE_SLOTS_FRACTION, MAX_RENDER_FRAMES, MAX_VOICE_OUT_BYTES, PendingReadback,
    STATES_SYNC_EVERY, VoiceDebugInfo, VoiceTemplateCache, checkpoint_ok, report_progress,
    spawn_budget_allows,
};
use voice_alloc::{select_damper_release_groups, select_evictions};

pub use types::{RenderCheckpoint, RenderProgress, RenderProgressFn, RenderResult};

/// A 10 ms linear-smoothed controller value (mirror of XSynth's `ValueLerp`).
#[derive(Debug, Clone, Copy)]
struct LerpState {
    /// Absolute frame of the last `advance_to` call.
    frame: u64,
    current: f32,
    end: f32,
    step: f32,
}

/// Currently-selected RPN/NRPN parameter (target of a following CC6/CC38
/// Data Entry). `None` means no parameter is armed, so a stray Data Entry
/// is ignored (as required by the MIDI spec).
#[derive(Clone, Copy, Debug, Default)]
enum ParamSel {
    #[default]
    None,
    /// Registered Parameter Number: `(msb, lsb)`.
    Rpn(u8, u8),
    /// Non-Registered Parameter Number: `(msb, lsb)`.
    Nrpn(u8, u8),
}

/// Per-channel MIDI state.
#[derive(Debug)]
struct ChannelState {
    program: u8,
    volume: LerpState,
    expression: LerpState,
    pan: LerpState,
    damper: bool,
    pitch_multiplier: f32,
    /// CC73 (attack) value affecting voices of this channel.
    env_attack: Option<u8>,
    /// CC72 (release) value affecting voices of this channel.
    env_release: Option<u8>,
    // ---- Pitch state (driven by Pitch Bend + RPN 0/1/2) ----
    /// Last 14-bit pitch-bend value (0..16383, center 8192).
    bend_value: i32,
    /// Pitch-bend sensitivity in semitones, set via RPN 0 (default 2.0, GM).
    bend_sensitivity: f32,
    /// Channel fine tuning in cents (RPN 1, 14-bit, center = 0).
    fine_cents: f32,
    /// Channel coarse tuning in cents (RPN 2, MSB semitones, center = 0).
    coarse_cents: f32,
    /// Parameter currently selected for Data Entry (RPN/NRPN).
    param: ParamSel,
    /// RPN 0 (pitch-bend sensitivity) 的 Data Entry 字节，按参数分槽存储：
    /// 切换 RPN 时旧参数的字节不会混入新参数（对齐 MIDI 规范与 CPU 实现）。
    rpn0_msb: u8,
    rpn0_lsb: u8,
    /// RPN 1 (fine tuning) 的 Data Entry 字节，标准 14-bit 中心为 8192
    /// （MSB=64, LSB=0）。
    rpn1_msb: u8,
    rpn1_lsb: u8,
}

/// The GPU-accelerated MIDI synthesizer.
///
/// # Example
///
/// ```no_run
/// use lumino_gpu_synth::{GpuSynth, SynthConfig};
///
/// let mut synth = GpuSynth::new(SynthConfig::default())?;
/// synth.load_soundfont("assets/test.sf2", 0, 0)?;
/// let result = synth.render_midi_file("assets/right-example.mid")?;
/// # Ok::<(), lumino_gpu_synth::SynthError>(())
/// ```
pub struct GpuSynth {
    config: SynthConfig,
    res: GpuResources,
    sf: Option<SoundFont>,

    // GPU buffers
    params_buf: GrowableBuffer,
    /// Resampled sample data, split across several capped chunks so no
    /// single storage binding exceeds the 128 MiB limit (D3D12).
    samples_chunks: Vec<GrowableBuffer>,
    sinc_buf: wgpu::Buffer,
    env_buf: GrowableBuffer,
    states_buf: GrowableBuffer,
    /// Per-voice output, grown on demand so dense MIDI never runs out of
    /// voice slots (the pool is a *physical* limit, not a polyphony one).
    voice_out_buf: GrowableBuffer,
    out_storage_buf: wgpu::Buffer,
    /// Double-buffered readback so the CPU can wait for the *previous*
    /// submission while the current one is still running on the GPU
    /// (CPU/GPU pipelining).
    out_readback: [wgpu::Buffer; 2],
    out_readback_cur: usize,
    /// Double-buffered voice-state readback: the copy lands in one buffer,
    /// the map reads the other (states from several blocks ago), so the
    /// wait only ever needs to cover already-completed work.
    states_readback: [GrowableBuffer; 2],
    states_readback_cur: usize,
    /// Per-voice channel ids, grown like `voice_out_buf`.
    voice_chans_buf: GrowableBuffer,
    /// Per-block controller events (frame-exact, replayed by the mix pass).
    mix_events_buf: GrowableBuffer,
    mix_params_buf: wgpu::Buffer,

    render_bg: Option<wgpu::BindGroup>,
    mix_bg: Option<wgpu::BindGroup>,
    render_bg_dirty: bool,
    mix_bg_dirty: bool,

    // State
    channels: Vec<ChannelState>,
    voices: Vec<Voice>,
    /// Per-(channel,key) positions of active voices, rebuilt after every
    /// voice-list mutation (`retain`) so note-on/note-off handling is O(1)
    /// instead of scanning the whole voice list (dense MIDI can hold tens
    /// of thousands of voices and millions of note events).
    /// Flat array indexed by `ch*128+key` (2048 entries) to avoid HashMap
    /// hashing overhead on the hot black-MIDI path (measured: ~30% of
    /// `apply` time on 20k-voice blocks).
    key_voices: Vec<VecDeque<usize>>, // len 2048
    sample_offsets: std::collections::HashMap<usize, (u32, u32)>, // sample_id -> (offset, len)
    samples_next_offset: u32,
    global_frame: u64,
    pending_events: VecDeque<TimedEvent>,
    offline_events: Vec<TimedEvent>,
    offline_cursor: usize,
    /// Volume/expression/pan CC events deferred to the mix stage so they are
    /// applied at their exact frame (not at the block boundary): a tuple of
    /// `(sample, channel, controller, value)`.
    pending_mix_events: Vec<(u64, u8, u8, u8)>,
    active_voice_count: u32,
    /// Output peak limiter gain (applied on the CPU side, after readback).
    ///
    /// The mix pass sums every active voice with no headroom management, so
    /// extreme instantaneous polyphony (hundreds to thousands of voices)
    /// peaks far past full scale (64 voices ~10x, 4096 ~600x). The output
    /// must be throttled, but a per-sample soft clip would have to squeeze
    /// the whole 1.0..~700 range into the 1.0..1.05 band, flat-topping the
    /// waveform into square-wave distortion (worse than clipping, verified
    /// empirically). Instead a block-level limiter scales the WHOLE block
    /// by a scalar gain: the waveform is preserved exactly, the peak lands
    /// at ~0.98, and the gain recovers exponentially (see `apply_limiter`).
    ///
    /// Attack is immediate: this block's gain is set from THIS block's peak
    /// (the data is already in hand at readback), so an overloaded block is
    /// throttled from its first sample - there is no attack-lag window
    /// leaking raw sums to the listener. Release is a ~50 ms exponential
    /// recovery so the volume returns without pumping or block-step clicks.
    limiter_gain: f32,
    /// Tail of the previous block's raw (pre-limiter) samples, used as the
    /// delay line head by the lookahead limiter (see `apply_limiter`): the
    /// output at block start is the delayed sample from the previous block,
    /// so the 1 ms delay is continuous across blocks.
    limiter_tail: Vec<f32>,
    // Readback staging (filled by dispatch, consumed by readback/sync).
    last_out: Option<Vec<u8>>,
    last_states: Option<Vec<u8>>,
    /// Voice ids of the last uploaded voice list, in upload order; used to
    /// map the read-back states onto the current (possibly shrunk) list.
    prev_voice_ids: Vec<u32>,
    /// Reused per-block upload buffers (avoid re-allocating ~1.5 MB of
    /// voice parameters every block when the pool sits at the cap).
    upload_params: Vec<VoiceParams>,
    upload_states: Vec<VoiceState>,
    upload_env_stages: Vec<EnvStageGpu>,
    upload_chans: Vec<u32>,
    /// Monotonic note-on counter; every zone voice of one note-on shares
    /// the current value as its `note_id`.
    note_counter: u64,
    /// Monotonic voice id counter. Voice ids must be unique for the lifetime
    /// of the engine: `upload_voices` maps read-back GPU states back to
    /// voices by id, and reusing ids (e.g. the array position) would apply a
    /// stale state to the wrong voice and roll its envelope back.
    voice_id_counter: u32,
    /// Per-(channel, key) note-on guard for the current block: pathological
    /// bursts beyond `MAX_SPAWNS_PER_KEY_PER_BLOCK` are skipped so one key
    /// cannot stall the render thread on an absurd event storm. This is NOT
    /// the musical per-key polyphony limit - that is enforced by
    /// `trim_key_voices` after spawning (XSynth semantics: every note-on
    /// sounds, the quietest old group is stolen). Using `max_voices_per_key`
    /// here dropped the NEWEST notes and broke dense passages (measured: 18%
    /// of a black MIDI's note-ons dropped at limit=4).
    spawn_budget: [u32; 16 * 128],
    /// Per-(channel, key) count of active (not ended, not released) note
    /// groups, so `release_key` can bail out in O(1) when a note-off has no
    /// target - black-MIDI peaks fire hundreds of thousands of orphan
    /// note-offs per block whose keys have no live notes left. Rebuilt
    /// exactly once per block in `upload_voices`; in-block spawns/releases
    /// adjust it, trims may leave it slightly stale (only costs a scan).
    /// u32：无限层数下同键活组可远超 u8(255)；u8 截断归零会让 release_key
    /// 跳过 note-off（挂音/超时），故不再截断。
    active_notes: [u32; 16 * 128],
    /// Voice template cache: `(key, vel, channel, pitch_mult_bits,
    /// env_attack, env_release)` -> pre-built voices for every zone of that
    /// note. Black-MIDI note storms spawn thousands of identical notes per
    /// block; cloning a template skips the soundfont zone lookup and the
    /// per-zone envelope-stage computation (the dominant CPU cost of
    /// `spawn_voices`).
    voice_templates: VoiceTemplateCache,
    /// Voice states are only read back every `STATES_SYNC_EVERY` blocks:
    /// a voice ending late does not change any audio sample, and skipping
    /// the extra map/poll round trip per block is a large CPU win.
    states_sync_counter: u32,
    /// One-block CPU/GPU pipeline state: the submission dispatched by the
    /// most recent `render_block`, plus the exact double-buffer slots its
    /// output and voice states were copied into. The NEXT `render_block`
    /// consumes it (`collect_pending_readback`) at its start, so the GPU
    /// renders block N while the CPU maps block N-1's audio back - the
    /// per-block synchronous poll wait disappears. Recording the exact
    /// slots (instead of a separate "previous submission" marker) keeps the
    /// readback window correct even when silent blocks skip dispatching.
    pending: Option<PendingReadback>,
    /// Persistent staging belt: reuses staging buffers across blocks so the
    /// per-block `queue.write_buffer` (which allocates + copies a fresh
    /// staging buffer every call) stops dominating the render time. Measured
    /// 35ms/block for ~400KB of voice uploads vs ~2ms with a belt.
    belt: wgpu::util::StagingBelt,
    /// Cooperative cancel/pause checkpoint for offline renders (see
    /// [`RenderCheckpoint`]); `None` = non-cancellable.
    render_checkpoint: Option<RenderCheckpoint>,
    /// 离线渲染进度回调（见 [`RenderProgressFn`]）；`None` = 不报告。
    render_progress: Option<RenderProgressFn>,
}

#[cfg(test)]
mod tests;
