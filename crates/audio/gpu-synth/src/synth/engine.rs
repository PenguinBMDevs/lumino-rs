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
const MAX_RENDER_FRAMES: u64 = 1 << 31;

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
const STATES_SYNC_EVERY: u32 = 1;

/// Headroom between the polyphony trim target and the physical GPU pool:
/// voices trimmed for polyphony fade out over 1 ms (one block), so the
/// pool must be able to hold the active voices PLUS one block's worth of
/// fading voices. `max_voices` stays the *active* polyphony target (and the
/// trim threshold); the pool is sized 1.5x so a sudden black-MIDI storm
/// fades everything out smoothly instead of hard-killing at the cap.
/// The fading voices are pruned by the next readback (they end after the
/// 1 ms fade), so the surplus is transient and the pool returns to
/// `max_voices` when the storm passes.
const FADE_SLOTS_FRACTION: usize = 2; // pool = max_voices * (1 + 1/FRACTION)

/// Hard cap for the voice output buffer (per-voice output for one block).
/// The wgpu/D3D12-style maximum buffer size is 2 GiB - 1; staying well
/// below it keeps headroom. `max_voices` must be chosen so the *peak*
/// active voice count stays under this (a dense MIDI may need 32k+).
/// For unlimited mode (max_voices == 0) the cap is effectively the device
/// maximum minus a small guard so black MIDI can use up to ~500k voices
/// per block at 512 frames (524k = (2 GiB - 64 KiB) / (512*8)).
const MAX_VOICE_OUT_BYTES: u64 = (1 << 31) - (1 << 16); // ~2 GiB - 64 KiB

/// A 10 ms linear-smoothed controller value (mirror of XSynth's `ValueLerp`).
#[derive(Debug, Clone, Copy)]
struct LerpState {
    /// Absolute frame of the last `advance_to` call.
    frame: u64,
    current: f32,
    end: f32,
    step: f32,
}

impl LerpState {
    fn new(initial: f32) -> Self {
        Self {
            frame: 0,
            current: initial,
            end: initial,
            step: 0.0,
        }
    }

    fn set_end(&mut self, end: f32, sample_rate: u32) {
        self.step = (end - self.current) / (sample_rate as f32 * 0.01);
        self.end = end;
    }

    /// Advances the lerp to the absolute `target` frame, returning the value
    /// at that point. Frame-exact, so the result does not depend on the
    /// block size.
    fn advance_to(&mut self, target: u64) -> f32 {
        let n = target.saturating_sub(self.frame);
        if n > 0 {
            self.frame = target;
            if self.end > self.current {
                self.current = (self.current + self.step * n as f32).min(self.end);
            } else if self.end < self.current {
                self.current = (self.current + self.step * n as f32).max(self.end);
            }
        }
        self.current
    }
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
    /// Most recent CC6 (Data Entry MSB) value.
    data_msb: u8,
    /// Most recent CC38 (Data Entry LSB) value.
    data_lsb: u8,
}

impl ChannelState {
    fn new() -> Self {
        Self {
            program: 0,
            volume: LerpState::new(1.0),
            expression: LerpState::new(1.0),
            pan: LerpState::new(0.5),
            damper: false,
            pitch_multiplier: 1.0,
            env_attack: None,
            env_release: None,
            bend_value: 8192,
            bend_sensitivity: 2.0,
            fine_cents: 0.0,
            coarse_cents: 0.0,
            param: ParamSel::None,
            data_msb: 0,
            data_lsb: 0,
        }
    }

    /// Recomputes `pitch_multiplier` from the current bend value/sensitivity
    /// and channel tuning (RPN 1/2).
    fn recompute_pitch(&mut self) {
        let bend_semitones = (self.bend_value as f32 - 8192.0) / 8192.0 * self.bend_sensitivity;
        let bend_mult = 2.0f32.powf(bend_semitones / 12.0);
        let tune_mult = 2.0f32.powf((self.fine_cents + self.coarse_cents) / 1200.0);
        self.pitch_multiplier = bend_mult * tune_mult;
    }

    /// Applies the current Data Entry bytes (`data_msb`/`data_lsb`) to the
    /// selected RPN. Only RPN 0 (pitch-bend sensitivity), RPN 1 (fine tuning)
    /// and RPN 2 (coarse tuning) affect pitch; NRPNs are left to the
    /// soundfont/synth-specific path.
    fn apply_rpn_data(&mut self) {
        let data = ((self.data_msb as u16) << 7) | (self.data_lsb as u16);
        match self.param {
            ParamSel::Rpn(0, 0) => {
                // Pitch Bend Sensitivity: MSB = semitones, LSB = fractional
                // semitone. Per GM the valid range is 0..24 semitones; clamp
                // defensively without starving legitimate wide settings.
                let semis = self.data_msb as f32 + self.data_lsb as f32 / 128.0;
                self.bend_sensitivity = semis.clamp(0.0, 96.0);
                self.recompute_pitch();
            }
            ParamSel::Rpn(0, 1) => {
                // Channel Fine Tuning: 14-bit, 8192 = 0 cents, range +-100 cents.
                self.fine_cents = ((data as f32 - 8192.0) / 8192.0) * 100.0;
                self.recompute_pitch();
            }
            ParamSel::Rpn(0, 2) => {
                // Channel Coarse Tuning: MSB = semitones, 64 = 0.
                self.coarse_cents = (self.data_msb as f32 - 64.0) * 100.0;
                self.recompute_pitch();
            }
            _ => {}
        }
    }
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
    /// Per-(channel, key) note-on budget for the current block. Black-MIDI
    /// peaks push tens of thousands of note-ons per key per block, of which
    /// only `max_voices_per_key` can survive the per-key trim - the rest are
    /// processing wasted on notes that produce no audible output. Once a
    /// key's budget is spent, further note-ons are skipped entirely. A flat
    /// array (16 channels x 128 keys) keeps the skip path a single indexed
    /// load - the HashMap version cost ~0.15us per skipped note-on.
    spawn_budget: [u8; 16 * 128],
    /// Per-(channel, key) count of active (not ended, not released) note
    /// groups, so `release_key` can bail out in O(1) when a note-off has no
    /// target - black-MIDI peaks fire hundreds of thousands of orphan
    /// note-offs per block whose keys have no live notes left. Rebuilt
    /// exactly once per block in `upload_voices`; in-block spawns/releases
    /// adjust it, trims may leave it slightly stale (only costs a scan).
    active_notes: [u8; 16 * 128],
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
}

/// A submission whose readback is still outstanding (see `GpuSynth::pending`).
struct PendingReadback {
    idx: wgpu::SubmissionIndex,
    /// `out_readback` slot the block's audio was copied to.
    out_slot: usize,
    /// `states_readback` slot the block's voice states were copied to.
    states_slot: usize,
}

/// Cache key for one note's voice templates (see `GpuSynth::voice_templates`).
type VoiceTemplateKey = (u8, u8, u8, u32, u8, u8);
/// Pre-built voices per note, keyed by (key, vel, channel, pitch_mult_bits,
/// env_attack, env_release).
type VoiceTemplateCache = std::collections::HashMap<VoiceTemplateKey, Vec<Voice>>;

/// Per-voice diagnostic row from [`GpuSynth::debug_voices`].
type VoiceDebugInfo = (u8, u8, f32, f32, bool, bool, u32, u32, u64, u32, f32);

/// Block limiter core, factored out of `GpuSynth::apply_limiter` so it can be
/// unit-tested without a GPU device.
///
/// Processes ONE stereo block (`out` is interleaved L,R,L,R...). `tail` is the
/// previous block's last `LOOKAHEAD` stereo frames (the delay-line head,
/// unscaled) carried across blocks; `gain` is the carried gain state. Both are
/// updated in place for the next block.
///
/// The limiter delays the signal by `LOOKAHEAD` frames and applies a lookahead
/// peak limiter so the signal never exceeds ~0.98. Crucially, the per-frame
/// gain peak-detects the EMITTED sample stream (which reaches into `tail` for a
/// spike that ended the previous block), so a brief single-sample transient is
/// attenuated at the exact output frame that outputs it - this is what stops the
/// "super-high short pop columns" that escaped the old forward-only window.
fn limit_block(out: &mut [f32], tail: &mut Vec<f32>, gain: &mut f32, sample_rate: f32) {
    const LOOKAHEAD: usize = 256; // frames; ~4 ms @ 64 kHz
    let n = out.len() / 2;
    if n == 0 {
        return;
    }
    let rate = sample_rate.max(1.0);

    // Pre-limiter mix (the whole block is in RAM, so the limiter can look
    // ahead). Sanitize non-finite samples first: NaN/Inf is a corrupted
    // artifact (e.g. a GPU compute blow-up at a chunk/segment boundary)
    // and must not leak a "super-high short pop" through the limiter or the
    // delay line - replace it with silence.
    let mut raw: Vec<f32> = out.to_vec();
    for s in raw.iter_mut() {
        if !s.is_finite() {
            *s = 0.0;
        }
    }

    // Peak of this block (for the fast path and the truncated tail).
    // `raw` is already sanitized above, so every sample is finite.
    let mut peak = 0.0f32;
    for &s in raw.iter() {
        let a = s.abs();
        peak = peak.max(a);
    }

    // `tail` holds the previous block's RAW input samples (the unscaled
    // delay-line head): the first LOOKAHEAD output frames are those samples
    // scaled by THIS block's gains - the gain applies at the output time,
    // exactly like the delay line itself, so the block boundary is seamless.
    if tail.len() < LOOKAHEAD * 2 {
        tail.resize(LOOKAHEAD * 2, 0.0);
    }

    let mut g = *gain;
    if peak <= 0.98 && g == 1.0 {
        // Fast path: nothing exceeds full scale AND the gain is fully
        // recovered - pure delay, no scaling.
        for i in 0..n {
            if i < LOOKAHEAD {
                out[i * 2] = tail[i * 2];
                out[i * 2 + 1] = tail[i * 2 + 1];
            } else {
                out[i * 2] = raw[(i - LOOKAHEAD) * 2];
                out[i * 2 + 1] = raw[(i - LOOKAHEAD) * 2 + 1];
            }
        }
        // The delay-line head for the next block is this block's RAW input
        // tail (unscaled): the next block scales it with ITS gain.
        tail.copy_from_slice(&raw[raw.len() - LOOKAHEAD * 2..]);
        *gain = 1.0;
        return;
    }

    let atk = 1.0 - (-1.0 / (0.0005 * rate)).exp(); // 0.5 ms attack
    let rel = 1.0 - (-1.0 / (0.080 * rate)).exp(); // 80 ms release
    let mut gains = vec![1.0f32; n];

    // Emitted-sample stream: output frame `f` emits the previous block's
    // delay-line `tail` for f < LOOKAHEAD, otherwise this block's `raw`
    // shifted by LOOKAHEAD. The gain for frame `i` MUST peak-detect THIS
    // stream around the sample it actually outputs (emit[i]) - NOT `raw`
    // alone: a spike sitting in the last LOOKAHEAD frames of the previous
    // block lives in `tail` and is emitted at a small `i` here, so the
    // window has to reach into `tail` to see it.
    let emit = |f: usize, t: &[f32], r: &[f32]| -> (f32, f32) {
        if f < LOOKAHEAD {
            (t[f * 2], t[f * 2 + 1])
        } else {
            (r[(f - LOOKAHEAD) * 2], r[(f - LOOKAHEAD) * 2 + 1])
        }
    };

    if peak > 0.98 {
        // Lookahead gain for EVERY frame i: g[i] approaches
        // 0.98 / (peak of emitted samples emit[i .. i + LOOKAHEAD]).
        //
        // The limiter delays the signal by LOOKAHEAD frames, so the sample
        // EMITTED at output frame `i` is emit[i]. Centring the window on
        // emit[i] (backward half reaches into `tail` for a previous-block
        // spike, forward half is true lookahead) guarantees a brief
        // transient is ducked BEFORE the frame that outputs it (anticipation).
        //
        // The previous window `raw[i+1 .. i+LOOKAHEAD]` looked ~2*LOOKAHEAD
        // AHEAD of the emitted sample and MISSED short spikes, so single-
        // sample "super-high pops" escaped the limiter entirely - the bug
        // behind the exported waveform's tall short pop columns. Both window
        // ends truncate at the block edge, where the block peak is the safe
        // fallback.
        for g_i in gains.iter_mut().enumerate() {
            let i = g_i.0;
            let mut l = 0.0f32;
            let start = i; // look AHEAD from the emitted sample (anticipation)
            let end = (i + LOOKAHEAD).min(n - 1);
            for f in start..=end {
                let (a, b) = emit(f, tail, &raw);
                l = l.max(a.abs()).max(b.abs());
            }
            // Forward window truncated at block end: next block unseen,
            // fall back to the block peak.
            if end < i + LOOKAHEAD {
                l = l.max(peak);
            }
            let target = if l > 0.98 { 0.98 / l } else { 1.0 };
            let k = if target < g { atk } else { rel };
            g += (target - g) * k;
            *g_i.1 = g;
        }
    } else {
        // Overloaded some time ago (g < 1) but this block is quiet:
        // the gain recovers exponentially; no lookahead scan needed.
        for g_i in gains.iter_mut() {
            g += (1.0 - g) * rel;
            *g_i = g;
        }
    }

    // Apply: output[i] = delayed raw input (previous block's tail for
    // i < LOOKAHEAD, else raw[i - LOOKAHEAD]) * gains[i], where gains[i]
    // is THIS block's gain at output time i.
    for i in 0..n {
        let (l, r) = if i < LOOKAHEAD {
            (tail[i * 2], tail[i * 2 + 1])
        } else {
            (raw[(i - LOOKAHEAD) * 2], raw[(i - LOOKAHEAD) * 2 + 1])
        };
        let gg = gains[i];
        // Final insurance for the truncated tail window: a soft knee,
        // never a hard flat-top.
        let vl = if gg == 1.0 { l } else { soft_knee(l * gg) };
        let vr = if gg == 1.0 { r } else { soft_knee(r * gg) };
        out[i * 2] = vl;
        out[i * 2 + 1] = vr;
    }
    tail.copy_from_slice(&raw[raw.len() - LOOKAHEAD * 2..]);
    *gain = g;
}

impl GpuSynth {
    /// Creates a new synthesizer with the given configuration, initializing
    /// the GPU device (a wgpu adapter is picked automatically).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::GpuInit`] if no GPU device can be created.
    pub fn new(config: SynthConfig) -> Result<Self, SynthError> {
        config.validate()?;
        let ctx = Arc::new(create_gpu_context()?);
        let res = GpuResources::new(ctx, config.block_size, config.max_voices)?;
        Self::with_resources(config, res)
    }

    /// Creates a synthesizer reusing an existing [`GpuResources`] (advanced
    /// use: multiple engines sharing one device).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Config`] if the configuration is invalid.
    pub fn with_resources(config: SynthConfig, res: GpuResources) -> Result<Self, SynthError> {
        config.validate()?;
        let device = &res.ctx.device;
        let queue = &res.ctx.queue;
        let block = config.block_size;
        let max_voices = config.max_voices;
        // Physical pool: active voices + room for one block's worth of
        // fading (trimmed) voices. See `FADE_SLOTS_FRACTION`.
        // When max_voices == 0 (unlimited / black-MIDI mode) the pool is
        // just an initial hint - buffers grow on demand without trimming.
        let pool = if max_voices == 0 {
            4096usize
        } else {
            max_voices + max_voices / FADE_SLOTS_FRACTION
        };

        let params_buf = GrowableBuffer::new(
            device,
            queue,
            "voice params",
            (VoiceParams::SIZE * pool) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let samples_chunks = (0..SAMPLES_CHUNKS)
            .map(|i| {
                GrowableBuffer::with_max_capacity(
                    device,
                    queue,
                    &format!("samples chunk {i}"),
                    1 << 20,
                    SAMPLES_CHUNK_BYTES,
                    wgpu::BufferUsages::STORAGE,
                )
            })
            .collect::<Vec<_>>();
        let sinc = crate::synth::dsp::build_sinc_table();
        let sinc_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sinc table"),
            size: (sinc.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        res.ctx
            .queue
            .write_buffer(&sinc_buf, 0, bytemuck::cast_slice(&sinc));

        let env_buf = GrowableBuffer::new(
            device,
            queue,
            "env stages",
            (EnvStageGpu::SIZE * pool * 8) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let states_buf = GrowableBuffer::new(
            device,
            queue,
            "voice states",
            (VoiceState::SIZE * pool) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let voice_out_buf = GrowableBuffer::with_max_capacity(
            device,
            queue,
            "voice out",
            (pool * block * 2 * 4) as u64,
            MAX_VOICE_OUT_BYTES,
            wgpu::BufferUsages::STORAGE,
        );
        let out_storage_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out storage"),
            size: (block * 2 * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let out_readback = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out readback 0"),
                size: (block * 2 * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out readback 1"),
                size: (block * 2 * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        ];
        // Zero the output storage and readback buffers: wgpu does not zero
        // them, and any window where a readback is consumed before its copy
        // (e.g. the first block of a session) would feed uninitialized
        // garbage to the audio output (measured: recurring single-sample
        // pops of ~40000).
        {
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            enc.clear_buffer(&out_storage_buf, 0, Some((block * 2 * 4) as u64));
            enc.clear_buffer(&out_readback[0], 0, Some((block * 2 * 4) as u64));
            enc.clear_buffer(&out_readback[1], 0, Some((block * 2 * 4) as u64));
            queue.submit(Some(enc.finish()));
        }
        let states_readback = [
            GrowableBuffer::new(
                device,
                queue,
                "states readback 0",
                (VoiceState::SIZE * pool) as u64,
                wgpu::BufferUsages::MAP_READ,
            ),
            GrowableBuffer::new(
                device,
                queue,
                "states readback 1",
                (VoiceState::SIZE * pool) as u64,
                wgpu::BufferUsages::MAP_READ,
            ),
        ];
        let voice_chans_buf = GrowableBuffer::new(
            device,
            queue,
            "voice channels",
            (pool * 4) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let mix_events_buf = GrowableBuffer::new(
            device,
            queue,
            "mix events",
            16 << 10,
            wgpu::BufferUsages::STORAGE,
        );
        let mix_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mix params"),
            size: MixParams::SIZE as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Zero the dynamic storage buffers that are read every dispatch.
        let zero = vec![0u8; VoiceParams::SIZE * pool];
        if !zero.is_empty() {
            res.ctx.queue.write_buffer(params_buf.buffer(), 0, &zero);
        }
        let zero = vec![0u8; VoiceState::SIZE * pool];
        if !zero.is_empty() {
            res.ctx.queue.write_buffer(states_buf.buffer(), 0, &zero);
        }
        let zero = vec![0u8; EnvStageGpu::SIZE * pool * 8];
        if !zero.is_empty() {
            res.ctx.queue.write_buffer(env_buf.buffer(), 0, &zero);
        }
        let zero = vec![0u8; pool * 4];
        let mut voice_chans_buf = voice_chans_buf;
        if !zero.is_empty() {
            let _ = voice_chans_buf.write(&res.ctx.device, &res.ctx.queue, 0, &zero);
        }

        let mut engine = Self {
            config,
            res,
            sf: None,
            params_buf,
            samples_chunks,
            sinc_buf,
            env_buf,
            states_buf,
            voice_out_buf,
            out_storage_buf,
            out_readback,
            out_readback_cur: 0,
            states_readback,
            states_readback_cur: 0,
            voice_chans_buf,
            mix_events_buf,
            mix_params_buf,
            render_bg: None,
            mix_bg: None,
            render_bg_dirty: true,
            mix_bg_dirty: true,
            channels: (0..16).map(|_| ChannelState::new()).collect(),
            voices: Vec::new(),
            key_voices: vec![VecDeque::new(); 16 * 128],
            sample_offsets: std::collections::HashMap::new(),
            samples_next_offset: 0,
            global_frame: 0,
            pending_events: VecDeque::new(),
            offline_events: Vec::new(),
            offline_cursor: 0,
            pending_mix_events: Vec::new(),
            active_voice_count: 0,
            limiter_gain: 1.0,
            limiter_tail: Vec::new(),
            last_out: None,
            last_states: None,
            prev_voice_ids: Vec::new(),
            upload_params: Vec::new(),
            upload_states: Vec::new(),
            upload_env_stages: Vec::new(),
            upload_chans: Vec::new(),
            note_counter: 0,
            voice_id_counter: 0,
            spawn_budget: [0; 16 * 128],
            active_notes: [0; 16 * 128],
            voice_templates: std::collections::HashMap::new(),
            states_sync_counter: 0,
            pending: None,
            belt: wgpu::util::StagingBelt::new(1 << 20),
        };
        engine.rebuild_bind_groups();
        Ok(engine)
    }

    /// Returns the engine configuration.
    pub fn config(&self) -> &SynthConfig {
        &self.config
    }

    /// Returns the adapter info (for diagnostics).
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.res.ctx.adapter_info
    }

    /// Loads a soundfont and selects `bank`/`preset`.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::SoundFont`] if parsing fails or the preset is
    /// missing.
    pub fn load_soundfont(
        &mut self,
        path: impl AsRef<std::path::Path>,
        bank: u16,
        preset: u16,
    ) -> Result<(), SynthError> {
        let sf = SoundFont::load(path, bank, preset, self.config.use_effects)?;
        self.sf = Some(sf);
        Ok(())
    }

    /// Unloads the current soundfont.
    pub fn unload_soundfont(&mut self) {
        self.sf = None;
    }

    /// Returns the number of currently active voices.
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// Diagnostics: `(voices, released, ended)` - how many voices exist,
    /// how many have a release scheduled, and how many the GPU marked ended.
    #[doc(hidden)]
    pub fn debug_voice_lifecycle(&self) -> (usize, usize, usize) {
        let released = self
            .voices
            .iter()
            .filter(|v| v.released || v.release_at != u64::MAX)
            .count();
        let ended = self.voices.iter().filter(|v| v.state.ended != 0).count();
        (self.voices.len(), released, ended)
    }

    /// Diagnostics: details of the first voice's GPU state.
    #[doc(hidden)]
    pub fn debug_voice_state(&self) -> Option<(u32, u32, u32, u32, u64, u64)> {
        let v = self.voices.first()?;
        Some((
            v.state.is_released,
            v.state.ended,
            v.state.env_stage,
            v.state.env_t,
            v.release_at,
            v.start_at,
        ))
    }

    /// Diagnostics: per-voice `(key, vel, speed, amp, released, ended,
    /// env_stage, env_t, release_at, gpu_is_released, env_from)`.
    #[doc(hidden)]
    pub fn debug_voices(&self) -> Vec<VoiceDebugInfo> {
        self.voices
            .iter()
            .map(|v| {
                (
                    v.key,
                    v.vel,
                    v.speed,
                    v.amp,
                    v.released || v.release_at != u64::MAX,
                    v.state.ended != 0,
                    v.state.env_stage,
                    v.state.env_t,
                    v.release_at,
                    v.state.is_released,
                    v.state.env_from,
                )
            })
            .collect()
    }

    /// The number of frames rendered so far.
    pub fn rendered_frames(&self) -> u64 {
        self.global_frame
    }

    // ------------------------------------------------------------------
    // Real-time event injection
    // ------------------------------------------------------------------

    /// Queues a MIDI event (applied at the next block boundary).
    pub fn send_event(&mut self, channel: u8, event: MidiEvent) {
        self.pending_events.push_back(TimedEvent::from_event(
            self.global_frame as u32,
            channel.min(15),
            event,
        ));
    }

    /// Loads a full sample-accurate event stream for realtime playback.
    ///
    /// The render thread consumes it internally (like the offline renderer)
    /// by wall-clock-progressed `global_frame`, so the events never travel
    /// through the per-event channel — the only way to keep up with dense
    /// black-MIDI (hundreds of thousands of note events per second).
    pub fn set_events(&mut self, events: Vec<TimedEvent>) {
        self.offline_cursor = 0;
        self.offline_events = events;
    }

    /// Returns true when the loaded event stream has been fully consumed and
    /// no voice is still sounding (used by realtime playback to end the
    /// render thread on its own).
    pub fn stream_exhausted(&self) -> bool {
        if self.offline_cursor < self.offline_events.len() {
            return false;
        }
        self.voices.is_empty()
    }

    /// Convenience: sends a note-on.
    pub fn note_on(&mut self, channel: u8, key: u8, vel: u8) {
        self.send_event(channel, MidiEvent::NoteOn { key, vel });
    }

    /// Convenience: sends a note-off.
    pub fn note_off(&mut self, channel: u8, key: u8) {
        self.send_event(channel, MidiEvent::NoteOff { key });
    }

    /// Convenience: sends a control change.
    pub fn control_change(&mut self, channel: u8, controller: u8, value: u8) {
        self.send_event(channel, MidiEvent::ControlChange { controller, value });
    }

    // ------------------------------------------------------------------
    // Block rendering
    // ------------------------------------------------------------------

    /// Renders one block of `block_size` frames into `out` (interleaved
    /// L/R, length `block_size * channels`).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] on dispatch/readback failures.
    pub fn render_block(&mut self, out: &mut [f32]) -> Result<(), SynthError> {
        let block = self.config.block_size;
        let chs = self.output_channels();
        if out.len() < block * chs {
            return Err(SynthError::Config("output buffer too small".into()));
        }

        let prof = std::env::var("LUMINO_PROFILE").is_ok();
        let base = self.global_frame;

        let t0 = std::time::Instant::now();
        self.apply_events(base, base + block as u64)?;
        let t1 = std::time::Instant::now();

        // One-block pipeline: consume the previous block's readback BEFORE
        // the fast-path check, because a silent block still owes the
        // listener the previous block's audio (the GPU rendered it while we
        // prepared this block). The pending submission has had a full block
        // of CPU work to finish, so the poll returns immediately.
        self.collect_pending_readback()?;
        let t1b = std::time::Instant::now();

        // Fast path: no voices at all - the block is pure silence (the mix
        // pass would sum nothing). Advance the controller states so CC
        // smoothing stays continuous and skip the GPU round trip. Dense-
        // but-sparse MIDI spends a large fraction of its timeline in note
        // gaps, so this is a significant win.
        //
        // We did NOT dispatch this block, so the GPU state did not advance
        // and this block's own audio is silence; the output owed to the
        // listener is the audio of the last DISPATCHED block, collected
        // above - exactly one block old, which is this block's turn to play.
        // With nothing pending (two consecutive silent blocks) it is
        // silence. `last_states`/`prev_voice_ids` are left intact: they are
        // the resume state of the last dispatch, still valid because the GPU
        // state did not move.
        if self.voices.is_empty() {
            self.update_mix_params(base)?;
            if let Some(data) = self.last_out.take() {
                let count = (data.len() / 4).min(block * chs);
                out[..count].copy_from_slice(bytemuck::cast_slice(&data[..count * 4]));
                if count < block * chs {
                    out[count..block * chs].fill(0.0);
                }
            } else {
                out[..block * chs].fill(0.0);
            }
            // The replayed block is the last DISPATCHED block's audio, which
            // the limiter would have scaled at its own readback; scale it
            // again so the limiter state stays continuous either way.
            self.apply_limiter(&mut out[..block * chs]);
            self.global_frame += block as u64;
            if prof {
                eprintln!(
                    "[profile] block {}: silent skip",
                    self.global_frame / block as u64 - 1
                );
            }
            return Ok(());
        }

        // States from the same readback: apply the previous block's GPU
        // state to the CPU mirror BEFORE uploading this block's parameters,
        // so `upload_voices` (a) prunes voices that ended on the GPU and
        // (b) has a fresh `v.state` fallback when a voice is not present in
        // the read-back list. `sync_voice_states` does not consume
        // `prev_voice_ids` - that list must stay aligned with `last_states`
        // for `upload_voices`' resume matching.
        self.sync_voice_states();
        let t1c = std::time::Instant::now();

        self.upload_voices(base)?;
        let t2 = std::time::Instant::now();
        self.upload_new_samples()?;
        let t3 = std::time::Instant::now();
        self.update_mix_params(base)?;
        let t4 = std::time::Instant::now();
        self.dispatch(base)?;
        let t5 = std::time::Instant::now();
        // The output owed this block is the audio collected at its start
        // (the previous block's render).
        self.readback(out)?;
        let t6 = std::time::Instant::now();

        if prof && self.global_frame.is_multiple_of(block as u64 * 25) {
            let block_no = self.global_frame / block as u64;
            eprintln!(
                "[profile] block {block_no}: apply={}us collect={}us sync={}us upload={}us samples={}us mix={}us dispatch={}us readback={}us total={}us voices={}",
                (t1 - t0).as_micros(),
                (t1b - t1).as_micros(),
                (t1c - t1b).as_micros(),
                (t2 - t1c).as_micros(),
                (t3 - t2).as_micros(),
                (t4 - t3).as_micros(),
                (t5 - t4).as_micros(),
                (t6 - t5).as_micros(),
                (t6 - t0).as_micros(),
                self.voices.len()
            );
        }

        self.global_frame += block as u64;
        Ok(())
    }

    /// Renders a full MIDI file to memory, stopping once all voices have
    /// decayed below the silence threshold (mirroring XSynth's offline
    /// renderer).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Midi`] if the file cannot be parsed, or
    /// [`SynthError::Gpu`] on GPU failures.
    pub fn render_midi_file(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
    ) -> Result<RenderResult, SynthError> {
        self.render_midi_inner(midi_path, None)
    }

    /// Forces one full GPU render pass (upload + dispatch + readback) so the
    /// driver compiles the pipelines and the first *real* block does not pay
    /// a hundreds-of-milliseconds cold-start stall (which empties the audio
    /// queue and causes crackle on dense MIDI).
    ///
    /// Safe to call before any notes are played; it renders a silent block.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] on GPU failures.
    pub fn warm_gpu(&mut self) -> Result<(), SynthError> {
        // A single voice with amp 0 renders silently but exercises the full
        // pipeline (upload, dispatch, readback). Save/restore the global
        // frame so the warm-up does not advance the timeline.
        if self.sf.is_none() {
            return Ok(());
        }
        let saved_frame = self.global_frame;
        let saved_voices = std::mem::take(&mut self.voices);
        for q in self.key_voices.iter_mut() {
            q.clear();
        }

        // Borrow sf to build one minimal voice.
        let mut buf = vec![0.0f32; self.config.block_size * self.output_channels()];
        if let Some(sf) = self.sf.as_ref()
            && let Some(&zid) = sf.zones_at(60, 100).first()
            && let Some(mut v) = build_voice(
                sf,
                zid,
                60,
                100,
                0,
                0,
                self.config.sample_rate,
                1.0,
                None,
                None,
                self.config.envelope_curves,
            )
        {
            v.amp = 0.0; // silent
            v.id = self.voice_id_counter;
            self.voice_id_counter += 1;
            self.voices.push(v);
        }
        if self.voices.is_empty() {
            // No soundfont/zone; nothing to warm. Restore and return.
            self.global_frame = saved_frame;
            return Ok(());
        }

        self.upload_voices(0)?;
        self.upload_new_samples()?;
        self.update_mix_params(0)?;
        self.dispatch(0)?;
        let _ = self.readback(&mut buf);

        // Restore state: drop the warm-up voice and reset the timeline.
        self.voices = saved_voices;
        for q in self.key_voices.iter_mut() {
            q.clear();
        }
        for (i, v) in self.voices.iter().enumerate() {
            self.key_voices[v.channel as usize * 128 + v.key as usize].push_back(i);
        }
        self.global_frame = saved_frame;
        self.active_voice_count = 0;
        self.last_out = None;
        self.last_states = None;
        self.prev_voice_ids.clear();
        Ok(())
    }

    /// Pre-builds the voice template cache for every (key, vel, channel)
    /// the MIDI uses, so the first dense blocks of realtime playback do not
    /// pay the per-note zone lookup + envelope build cost (which can spike
    /// the render load of the opening blocks of black-MIDI).
    fn warm_voice_templates(&mut self, events: &[TimedEvent]) {
        let Some(sf) = self.sf.as_ref() else {
            return;
        };
        let rate = self.config.sample_rate;
        let curves = self.config.envelope_curves;
        // Bitmap over the (channel x key x vel) grid: the HashMap lookup per
        // event cost ~50ns x 200M events on black-MIDI; this is O(1) array
        // indexing. Templates are built for the DEFAULT channel state only
        // (pitch 1.0, no env CC) - notes with bends or CC72/73 fall back to
        // building on demand.
        let mut seen: Vec<u8> = vec![0; 16 * 128 * 128];
        for ev in events {
            let MidiEvent::NoteOn { key, vel } = ev.event() else {
                continue;
            };
            let slot =
                &mut seen[ev.channel() as usize * 128 * 128 + key as usize * 128 + vel as usize];
            if *slot != 0 {
                continue;
            }
            *slot = 1;
            let tmpl_key = (key, vel, ev.channel(), 1.0f32.to_bits(), 0xFF, 0xFF);
            if self.voice_templates.contains_key(&tmpl_key) {
                continue;
            }
            let mut built: Vec<Voice> = Vec::new();
            for &zid in sf.zones_at(key, vel) {
                if let Some(v) = build_voice(
                    sf,
                    zid,
                    key,
                    vel,
                    ev.channel(),
                    0,
                    rate,
                    1.0,
                    None,
                    None,
                    curves,
                ) {
                    built.push(v);
                }
            }
            if !built.is_empty() {
                self.voice_templates.insert(tmpl_key, built);
            }
        }
    }

    /// Pre-warms the GPU sample cache with every sample the MIDI file will
    /// use (resampled and uploaded up front).
    ///
    /// Use this before realtime playback so the render loop never stalls on
    /// a lazily-resampled sample during dense sections — otherwise a single
    /// large sample can take hundreds of milliseconds to resample+upload in
    /// the middle of a block, emptying the audio queue and causing crackle.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Midi`] if the file cannot be parsed, or
    /// [`SynthError::Gpu`] on GPU failures.
    pub fn prewarm_midi_file(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
    ) -> Result<(), SynthError> {
        let t0 = std::time::Instant::now();
        let midi = MidiFile::load(midi_path, self.config.sample_rate)?;
        let events = &midi.sequence.events;
        if let Some(sf) = self.sf.as_ref() {
            // One pass over the (possibly 100M+ event) stream. Black-MIDI
            // repeats the same (key, vel) millions of times, so a bitmap of
            // the 128x128 key/velocity grid skips `zones_at` for repeats -
            // the previous version called `zones_at` per event (100M+ calls)
            // and scanned the stream TWICE (samples + templates), which is
            // why prewarming "Rekt Apple!!.mid" took 90+ seconds.
            let mut seen: Vec<u8> = vec![0; 128 * 128];
            let mut wanted: Vec<usize> = Vec::new();
            for ev in events {
                let MidiEvent::NoteOn { key, vel } = ev.event() else {
                    continue;
                };
                let slot = &mut seen[key as usize * 128 + vel as usize];
                if *slot != 0 {
                    continue;
                }
                *slot = 1;
                for &zid in sf.zones_at(key, vel) {
                    let zone = sf.zone(zid);
                    wanted.push(zone.sample_id);
                    wanted.push(zone.sample_id_r);
                }
            }
            wanted.sort_unstable();
            wanted.dedup();
            let rate = self.config.sample_rate;
            let pre: Vec<(usize, Arc<[f32]>)> = wanted
                .par_iter()
                .map(|&id| (id, sf.resample_uncached(id, rate)))
                .collect();
            let sf = self.sf.as_mut().expect("soundfont present");
            let device = &self.res.ctx.device;
            let queue = &self.res.ctx.queue;
            let mut grown = false;
            for (id, data) in pre {
                sf.cache_resampled(id, rate, data.clone());
                let len = data.len() as u32;
                let offset = self.samples_next_offset;
                grown |= write_samples(
                    &mut self.samples_chunks,
                    device,
                    queue,
                    offset as u64 * 4,
                    bytemuck::cast_slice(&data),
                )?;
                self.sample_offsets.insert(id, (offset, len));
                self.samples_next_offset = offset + len;
            }
            if grown {
                self.render_bg_dirty = true;
            }
        }
        // Warm the GPU pipelines so the first realtime block does not pay a
        // multi-hundred-ms cold start (which would empty the audio queue).
        self.warm_gpu()?;
        // Pre-build voice templates so the opening blocks do not pay the
        // per-note build cost either.
        self.warm_voice_templates(events);
        let t1 = std::time::Instant::now();
        if std::env::var("LUMINO_PROFILE").is_ok() {
            eprintln!(
                "[prewarm] load+scan+upload: {:.1}s, templates: {:.1}s",
                (t1 - t0).as_secs_f64(),
                t1.elapsed().as_secs_f64()
            );
        }
        Ok(())
    }

    /// Renders the first `frames` frames of a MIDI file (used to compare the
    /// beginning of long MIDIs without rendering the whole piece).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Midi`] if the file cannot be parsed, or
    /// [`SynthError::Gpu`] on GPU failures.
    pub fn render_midi_frames(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        frames: u64,
    ) -> Result<RenderResult, SynthError> {
        self.render_midi_inner(midi_path, Some(frames))
    }

    fn render_midi_inner(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        limit_frames: Option<u64>,
    ) -> Result<RenderResult, SynthError> {
        self.offline_cursor = 0;
        self.offline_events = Vec::new();
        self.voices.clear();
        self.global_frame = 0;
        self.active_voice_count = 0;
        self.last_states = None;
        self.last_out = None;
        self.prev_voice_ids.clear();
        self.pending = None;

        let prof = std::env::var("LUMINO_PROFILE").is_ok();
        let t0 = std::time::Instant::now();
        let midi = MidiFile::load(midi_path, self.config.sample_rate)?;
        let t1 = std::time::Instant::now();
        self.offline_events = midi.sequence.events;

        // Pre-warm resampling AND upload: resolve every sample the MIDI will
        // use, resample it in parallel and upload it to the GPU up front, so
        // the render loop never stalls on a lazily-resampled sample or pays
        // per-block sample uploads during the dense sections.
        if let Some(sf) = self.sf.as_ref() {
            let mut wanted: Vec<usize> = Vec::new();
            for ev in &self.offline_events {
                if let MidiEvent::NoteOn { key, vel } = ev.event() {
                    for &zid in sf.zones_at(key, vel) {
                        let zone = sf.zone(zid);
                        wanted.push(zone.sample_id);
                        wanted.push(zone.sample_id_r);
                    }
                }
            }
            wanted.sort_unstable();
            wanted.dedup();
            let rate = self.config.sample_rate;
            let pre: Vec<(usize, Arc<[f32]>)> = wanted
                .par_iter()
                .map(|&id| (id, sf.resample_uncached(id, rate)))
                .collect();
            let sf = self.sf.as_mut().expect("soundfont present");
            let device = &self.res.ctx.device;
            let queue = &self.res.ctx.queue;
            let mut grown = false;
            for (id, data) in pre {
                sf.cache_resampled(id, rate, data.clone());
                let len = data.len() as u32;
                let offset = self.samples_next_offset;
                grown |= write_samples(
                    &mut self.samples_chunks,
                    device,
                    queue,
                    offset as u64 * 4,
                    bytemuck::cast_slice(&data),
                )?;
                self.sample_offsets.insert(id, (offset, len));
                self.samples_next_offset = offset + len;
            }
            if grown {
                self.render_bg_dirty = true;
            }
        }

        // Render timeout guard: the offline loops must terminate on their own
        // (events consumed + silence / no voices). A voice that can never
        // finish - held damper, missing note-off, pathological envelope -
        // would otherwise loop forever. Abort once the last event is behind
        // us by `max_tail_seconds`; the hard cap keeps the guard well inside
        // the u32 frame range used by the GPU parameters.
        let events_end = self.offline_events.last().map_or(0, |e| e.sample as u64);
        let tail_budget =
            (self.config.max_tail_seconds as f64 * self.config.sample_rate as f64) as u64;
        let max_frames = match limit_frames {
            Some(n) => n.min(MAX_RENDER_FRAMES),
            None => events_end
                .saturating_add(tail_budget)
                .min(MAX_RENDER_FRAMES),
        };
        let limited = limit_frames.is_some();

        let block = self.config.block_size;
        let chs = self.output_channels();
        let threshold = self.config.render_silence_threshold;
        let mut samples: Vec<f32> = Vec::new();
        let mut block_buf = vec![0.0f32; block * chs];

        // Progress reporting: the total is the render horizon (`max_frames`).
        // Phase 1 walks the event stream; the tail phase renders past the
        // last event, so the bar is allowed to exceed 100% there.
        let mut progress = ProgressBar::new(max_frames, self.config.show_progress);

        // The one-block pipeline makes every `render_block` output the audio
        // of the PREVIOUS dispatched block. The very first block therefore
        // emits the fake "-1" audio (silence, nothing was dispatched before
        // it) which must not enter the sample stream; block 0's real audio
        // arrives with block 1.
        let mut first_block = true;

        // Phase 1: process all events and render until no voices remain. If
        // the events are exhausted and the block went silent, we stop even
        // when voices linger (they are stuck in sustain and contribute
        // nothing; the tail below would be silent too).
        //
        // NOTE on the appending order: with the one-block pipeline every
        // `render_block` outputs the audio of the block it dispatched
        // PREVIOUSLY, so the frame range of `block_buf` lags `global_frame`
        // by one block. Appending must therefore happen BEFORE the
        // `max_frames` check - the block being appended is the one that
        // just crossed (or reached) the limit, i.e. the last in-limit
        // audio. Checking first would drop it and let the drain append an
        // out-of-limit block instead, shifting the whole tail by one block.
        loop {
            let events_done = self.offline_cursor >= self.offline_events.len();
            if events_done && self.voices.is_empty() {
                if prof {
                    eprintln!(
                        "[render] break: events_done+empty at frame {}",
                        self.global_frame
                    );
                }
                break;
            }
            let rb_t0 = std::time::Instant::now();
            self.render_block(&mut block_buf)?;
            let rb_dt = rb_t0.elapsed();
            progress.tick(self.global_frame);
            let silent = block_buf.iter().all(|s| s.abs() <= threshold);
            if rb_dt.as_millis() > 30 {
                eprintln!(
                    "[slow-block] frame={} render={:?} silent={} cursor={} voices={}",
                    self.global_frame,
                    rb_dt,
                    silent,
                    self.offline_cursor,
                    self.voices.len()
                );
            }
            if events_done && silent {
                if prof {
                    eprintln!(
                        "[render] break: events_done+silent at frame {} cursor={}/{}",
                        self.global_frame,
                        self.offline_cursor,
                        self.offline_events.len()
                    );
                }
                break;
            }
            if !first_block {
                samples.extend_from_slice(&block_buf);
            }
            first_block = false;
            if self.global_frame >= max_frames {
                if limited {
                    break;
                }
                return Err(self.render_timeout(&block_buf));
            }
        }

        // Phase 2: decay tail - render blocks until one is entirely silent.
        loop {
            self.render_block(&mut block_buf)?;
            progress.tick(self.global_frame);
            let silent = block_buf.iter().all(|s| s.abs() <= threshold);
            if silent {
                break;
            }
            if self.global_frame >= max_frames {
                if !limited {
                    return Err(self.render_timeout(&block_buf));
                }
                // Frame-limited: this output is the audio of the block that
                // crossed the limit (one block behind `global_frame`);
                // append it only if it still starts inside the limit, then
                // stop - rendering any further would only produce
                // out-of-limit audio.
                if (self.global_frame - block as u64) < max_frames {
                    samples.extend_from_slice(&block_buf);
                }
                break;
            }
            samples.extend_from_slice(&block_buf);
        }

        // Drain the pipeline: the data of the last submitted block is only
        // read back by one more render. Append it if it is not silence
        // (the loops above already consumed every non-silent block).
        self.render_block(&mut block_buf)?;
        if block_buf.iter().any(|s| s.abs() > threshold) {
            samples.extend_from_slice(&block_buf);
        }
        progress.finish();

        if prof {
            let t2 = std::time::Instant::now();
            eprintln!(
                "[profile] midi load: {:?}, render loops: {:?}, flush: {:?}",
                t1 - t0,
                t2 - t1,
                t2.elapsed()
            );
        }

        let frames = (samples.len() / chs) as u64;
        Ok(RenderResult {
            samples,
            sample_rate: self.config.sample_rate,
            channels: chs as u32,
            frames,
        })
    }

    /// Builds the error reported when offline rendering exceeds its frame
    /// budget (a voice never finished).
    fn render_timeout(&self, last_block: &[f32]) -> SynthError {
        let last_peak = last_block.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        SynthError::RenderTimeout {
            frames: self.global_frame,
            active_voices: self.voices.len(),
            last_peak,
        }
    }

    const MAX_CPU_MEM_BYTES: u64 = 100 * 1024 * 1024;

    fn check_memory(&self) -> Result<(), SynthError> {
        // Heuristic MIDI budget — file + Vec<TimedEvent> must stay <100 MB.
        // With true file streaming the heap is O(tracks + block); voices dominate.
        let voices_mem =
            (self.voices.len() * std::mem::size_of::<crate::synth::voices::Voice>()) as u64;
        let upload_mem = (self.upload_params.len() * std::mem::size_of::<crate::gpu::VoiceParams>()
            + self.upload_states.len() * std::mem::size_of::<crate::gpu::VoiceState>())
            as u64;
        let total = voices_mem + upload_mem + 8 * 1024 * 4; // 4 tracks × 8 KiB
        if total > Self::MAX_CPU_MEM_BYTES {
            return Err(SynthError::Config(format!(
                "MIDI/CPU budget {} bytes exceeds 100 MB (voices {} upload {} KiB)",
                total,
                self.voices.len(),
                upload_mem / 1024
            )));
        }
        if self
            .global_frame
            .is_multiple_of(self.config.block_size as u64 * 25)
        {
            eprintln!(
                "[mem] midi≈{} MB voices={} upload≈{} KiB",
                total / (1024 * 1024),
                self.voices.len(),
                upload_mem / 1024
            );
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Streaming offline render — zero `Vec<TimedEvent>` / zero full-sample Vec
    // ------------------------------------------------------------------

    fn apply_events_streaming(
        &mut self,
        stream: &mut MidiStream,
        end: u64,
    ) -> Result<(), SynthError> {
        while let Some(ev) = self.pending_events.pop_front() {
            self.handle_event(ev)?;
        }
        while let Some(ev) = stream.peek() {
            if ev.sample as u64 >= end {
                break;
            }
            let ev = stream.next_event().expect("peeked event must exist");
            self.handle_event(ev)?;
        }
        Ok(())
    }

    fn render_block_streaming(
        &mut self,
        out: &mut [f32],
        stream: &mut MidiStream,
    ) -> Result<(), SynthError> {
        let block = self.config.block_size;
        let chs = self.output_channels();
        if out.len() < block * chs {
            return Err(SynthError::Config("output buffer too small".into()));
        }
        let base = self.global_frame;
        self.apply_events_streaming(stream, base + block as u64)?;
        self.collect_pending_readback()?;
        if self.voices.is_empty() {
            self.update_mix_params(base)?;
            if let Some(data) = self.last_out.take() {
                let count = (data.len() / 4).min(block * chs);
                out[..count].copy_from_slice(bytemuck::cast_slice(&data[..count * 4]));
                if count < block * chs {
                    out[count..block * chs].fill(0.0);
                }
            } else {
                out[..block * chs].fill(0.0);
            }
            self.apply_limiter(&mut out[..block * chs]);
            self.global_frame += block as u64;
            return Ok(());
        }
        self.sync_voice_states();
        self.upload_voices(base)?;
        self.upload_new_samples()?;
        self.update_mix_params(base)?;
        self.dispatch(base)?;
        self.readback(out)?;
        self.global_frame += block as u64;
        Ok(())
    }

    /// Streaming offline render that writes directly to `wav_path` without
    /// ever holding the MIDI event array or the full sample buffer in memory.
    ///
    /// The MIDI file is consumed via [`MidiStream`] (heap-merged, 8 bytes
    /// saved per event vs `MidiFile`) and audio is flushed block-by-block
    /// through [`crate::audio::wav::WavStreamWriter`]. Peak memory is
    /// therefore `O(tracks + block)` instead of `O(events + samples)`.
    pub fn render_midi_to_wav_streaming(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        wav_path: impl AsRef<std::path::Path>,
        limit_frames: Option<u64>,
    ) -> Result<RenderResult, SynthError> {
        // Reset state exactly like `render_midi_inner`
        self.offline_cursor = 0;
        self.offline_events = Vec::new();
        self.voices.clear();
        for q in self.key_voices.iter_mut() {
            q.clear();
        }
        self.spawn_budget = [0; 16 * 128];
        self.active_notes = [0; 16 * 128];
        self.global_frame = 0;
        self.active_voice_count = 0;
        self.last_states = None;
        self.last_out = None;
        self.prev_voice_ids.clear();
        self.pending = None;
        self.pending_events.clear();
        self.pending_mix_events.clear();

        let prof = std::env::var("LUMINO_PROFILE").is_ok();
        let t0 = std::time::Instant::now();
        let mut stream = MidiStream::open(midi_path.as_ref(), self.config.sample_rate)?;
        let t1 = std::time::Instant::now();
        // Pre-warm: collect wanted samples via raw track scan (O(n), no heap)
        // — same set as `render_midi_inner` but without consuming the stream,
        // so no `rewind` needed. The old heap-scan produced 299 sample diffs
        // when skipped (lazy per-block uploads race the pipeline).
        {
            let mut wanted: Vec<usize> = Vec::new();
            if let Some(sf_ref) = self.sf.as_ref() {
                stream.for_each_note_on(|key, vel| {
                    for &zid in sf_ref.zones_at(key, vel) {
                        let z = sf_ref.zone(zid);
                        wanted.push(z.sample_id);
                        wanted.push(z.sample_id_r);
                    }
                });
                wanted.sort_unstable();
                wanted.dedup();
            }
            if !wanted.is_empty() {
                let rate = self.config.sample_rate;
                // Chunked resample+upload to keep peak <100 MB (was holding all Arcs at once: 200 MB+)
                let mut grown = false;
                for chunk in wanted.chunks(16) {
                    let pre: Vec<(usize, Arc<[f32]>)> = if let Some(sf_ref) = self.sf.as_ref() {
                        chunk
                            .par_iter()
                            .map(|&id| (id, sf_ref.resample_uncached(id, rate)))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let Some(sf_mut) = self.sf.as_mut() else {
                        continue;
                    };
                    let device = &self.res.ctx.device;
                    let queue = &self.res.ctx.queue;
                    for (id, data) in pre {
                        sf_mut.cache_resampled(id, rate, data.clone());
                        let len = data.len() as u32;
                        let offset = self.samples_next_offset;
                        grown |= write_samples(
                            &mut self.samples_chunks,
                            device,
                            queue,
                            offset as u64 * 4,
                            bytemuck::cast_slice(&data),
                        )?;
                        self.sample_offsets.insert(id, (offset, len));
                        self.samples_next_offset = offset + len;
                    }
                }
                if grown {
                    self.render_bg_dirty = true;
                }
            }
        }

        let events_end = stream.end_sample();
        let tail_budget =
            (self.config.max_tail_seconds as f64 * self.config.sample_rate as f64) as u64;
        let max_frames = match limit_frames {
            Some(n) => n.min(MAX_RENDER_FRAMES),
            None => events_end
                .saturating_add(tail_budget)
                .min(MAX_RENDER_FRAMES),
        };
        let limited = limit_frames.is_some();
        let block = self.config.block_size;
        let chs = self.output_channels();
        let threshold = self.config.render_silence_threshold;
        let mut writer = crate::audio::wav::WavStreamWriter::create(
            wav_path.as_ref(),
            self.config.sample_rate,
            chs as u16,
        )?;
        let mut block_buf = vec![0.0f32; block * chs];
        let mut progress = ProgressBar::new(max_frames, self.config.show_progress);
        let mut first_block = true;

        // Phase 1: events + decay interleaved, streaming block by block
        loop {
            let events_done = stream.is_exhausted();
            if events_done && self.voices.is_empty() {
                if prof {
                    eprintln!(
                        "[render-stream] break: events_done+empty at frame {}",
                        self.global_frame
                    );
                }
                break;
            }
            let rb_t0 = std::time::Instant::now();
            self.render_block_streaming(&mut block_buf, &mut stream)?;
            let rb_dt = rb_t0.elapsed();
            progress.tick(self.global_frame);
            self.check_memory()?;
            let silent = block_buf.iter().all(|s| s.abs() <= threshold);
            if rb_dt.as_millis() > 30 {
                eprintln!(
                    "[slow-block-stream] frame={} render={:?} silent={} voices={}",
                    self.global_frame,
                    rb_dt,
                    silent,
                    self.voices.len()
                );
            }
            if events_done && silent {
                if prof {
                    eprintln!(
                        "[render-stream] break: events_done+silent at frame {}",
                        self.global_frame
                    );
                }
                break;
            }
            if !first_block {
                writer.write_samples(&block_buf)?;
            }
            first_block = false;
            if self.global_frame >= max_frames {
                if limited {
                    break;
                }
                return Err(self.render_timeout(&block_buf));
            }
        }

        // Phase 2: tail
        loop {
            self.render_block_streaming(&mut block_buf, &mut stream)?;
            progress.tick(self.global_frame);
            let silent = block_buf.iter().all(|s| s.abs() <= threshold);
            if silent {
                break;
            }
            if self.global_frame >= max_frames {
                if !limited {
                    return Err(self.render_timeout(&block_buf));
                }
                if (self.global_frame - block as u64) < max_frames {
                    writer.write_samples(&block_buf)?;
                }
                break;
            }
            writer.write_samples(&block_buf)?;
        }

        // Drain pipeline
        self.render_block_streaming(&mut block_buf, &mut stream)?;
        if block_buf.iter().any(|s| s.abs() > threshold) {
            writer.write_samples(&block_buf)?;
        }
        let frames = writer.frames_written();
        writer.finalize()?;
        progress.finish();

        if prof {
            let t2 = std::time::Instant::now();
            eprintln!(
                "[profile-stream] midi load: {:?}, render loops: {:?}, flush: {:?}",
                t1 - t0,
                t2 - t1,
                t2.elapsed()
            );
        }

        Ok(RenderResult {
            samples: Vec::new(),
            sample_rate: self.config.sample_rate,
            channels: chs as u32,
            frames,
        })
    }

    /// Convenience: streaming render of a whole file to `wav_path`.
    pub fn render_midi_file_to_wav_streaming(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        wav_path: impl AsRef<std::path::Path>,
    ) -> Result<RenderResult, SynthError> {
        self.render_midi_to_wav_streaming(midi_path, wav_path, None)
    }

    /// Convenience: streaming render of the first `frames` frames to `wav_path`.
    pub fn render_midi_frames_to_wav_streaming(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        wav_path: impl AsRef<std::path::Path>,
        frames: u64,
    ) -> Result<RenderResult, SynthError> {
        self.render_midi_to_wav_streaming(midi_path, wav_path, Some(frames))
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    fn output_channels(&self) -> usize {
        if self.config.channels == ChannelMode::Stereo {
            2
        } else {
            1
        }
    }

    fn apply_events(&mut self, _base: u64, end: u64) -> Result<(), SynthError> {
        // Real-time queue first.
        while let Some(ev) = self.pending_events.pop_front() {
            self.handle_event(ev)?;
        }
        // Offline event stream (events with sample < end belong to this block).
        while self.offline_cursor < self.offline_events.len() {
            let ev = self.offline_events[self.offline_cursor];
            if ev.sample as u64 >= end {
                break;
            }
            self.offline_cursor += 1;
            self.handle_event(ev)?;
        }
        Ok(())
    }

    fn handle_event(&mut self, ev: TimedEvent) -> Result<(), SynthError> {
        let ch = ev.channel() as usize;
        // Fast path on packed kind/payload to avoid constructing MidiEvent
        // enum for the hot note-on/note-off path (black MIDI: >1M events/sec).
        match ev.kind() {
            crate::midi::kind::NOTE_ON => {
                let payload = ev.payload();
                let key = payload as u8;
                let vel = (payload >> 8) as u8;
                // Velocity 0 is converted to a note-off by the parser; a
                // velocity of 1 is a barely-audible note that XSynth does
                // not render. Dropping it saves a voice slot without any
                // audible change.
                if vel <= 1 {
                    return Ok(());
                }
                // Per-key note-on budget, checked HERE (not inside
                // `spawn_voices`) so black-MIDI overflow notes skip the
                // function call entirely - the peaks fire hundreds of
                // thousands of note-ons per block, of which only
                // `max_voices_per_key` can survive the per-key trim.
                let limit = self.config.max_voices_per_key;
                if limit > 0 {
                    const BUDGET_MULT: u8 = 1;
                    let slot = &mut self.spawn_budget[ch * 128 + key as usize];
                    if *slot >= (limit as u8).saturating_mul(BUDGET_MULT) {
                        return Ok(());
                    }
                    *slot += 1;
                }
                // Global pool gate: bound per-block spawn cost WITHOUT dropping
                // audible notes. `upload_voices` trims the pool down to `pool`
                // keeping the OLDEST voices, so a new note-on must still be
                // spawned even when the pool is already full - it steals an
                // older voice and sounds. We therefore only stop spawning once
                // we are a full pool ABOVE the cap, which leaves enough
                // headroom for the per-key loudest-pick and the global
                // oldest-survives steal while capping event processing at
                // ~2*pool spawns/block instead of the unbounded note-on burst
                // that dominated `apply_events` (and starved the realtime
                // queue). Notes past the `2*pool` bound would be trimmed away
                // by the pool cap anyway, so nothing audible is lost.
                // In unlimited mode (max_voices == 0) every note must sound, so
                // the gate is disabled - but we still cap per-block spawns to
                // a very large budget (200k) to avoid a pathological 10M note
                // burst stalling the render thread for seconds.
                if self.config.max_voices != 0 {
                    let pool =
                        self.config.max_voices + self.config.max_voices / FADE_SLOTS_FRACTION;
                    if self.voices.len() >= pool + pool {
                        return Ok(());
                    }
                } else if self.voices.len() >= 400_000 {
                    // Unlimited but still bound the worst-case single-block burst
                    // to keep the block time bounded; black MIDI peaks are far
                    // below this, so nothing audible is lost.
                    return Ok(());
                }
                self.spawn_voices(ch, key, vel, ev.sample as u64)
            }
            crate::midi::kind::NOTE_OFF => {
                let key = ev.payload() as u8;
                self.release_key(ch, key, ev.sample as u64)
            }
            crate::midi::kind::CONTROL_CHANGE => {
                let payload = ev.payload();
                let controller = payload as u8;
                let value = (payload >> 8) as u8;
                match controller {
                    // Channel mix controllers: deferred for frame-exact
                    // application at the mix stage.
                    0x07 | 0x0B | 0x0A | 0x08 => {
                        self.pending_mix_events.push((
                            ev.sample as u64,
                            ch as u8,
                            controller,
                            value,
                        ));
                    }
                    _ => self.apply_cc(ch, controller, value),
                }
                Ok(())
            }
            crate::midi::kind::PROGRAM_CHANGE => {
                let program = ev.payload() as u8;
                self.channels[ch].program = program.min(127);
                Ok(())
            }
            crate::midi::kind::PITCH_BEND => {
                let value = ev.payload() as u16;
                // 14-bit value: 0..16383, center 8192. The sensitivity comes
                // from RPN 0 (Pitch Bend Sensitivity), defaulting to 2
                // semitones; store the raw value and recompute so a later RPN
                // change re-scales the held bend correctly.
                self.channels[ch].bend_value = value as i32;
                self.channels[ch].recompute_pitch();
                self.propagate_channel_pitch(ch);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn release_key(&mut self, ch: usize, key: u8, at: u64) -> Result<(), SynthError> {
        // O(1) bail-out for orphan note-offs (no active note group on this
        // key): black-MIDI peaks fire hundreds of thousands of these per
        // block and the key scan below would dominate the block time.
        // Flame graph: `std::env::var` per note-off cost ~12% of apply
        // (200k calls/block). Cache it once.
        static NO_ACTIVE_BYPASS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let bypass = *NO_ACTIVE_BYPASS.get_or_init(|| std::env::var("LUMINO_NO_ACTIVE").is_ok());
        if !bypass && self.active_notes[ch * 128 + key as usize] == 0 {
            return Ok(());
        }
        let damper = self.channels[ch].damper;
        // Indexed by (channel, key): only the voices of this key are touched.
        //
        // XSynth releases exactly one note per NoteOff - the *oldest* note
        // not yet releasing (FIFO, `release_next_voice`), and it releases
        // the whole note *group* (all zone voices spawned by that note-on)
        // at once. Releasing every voice of the key would cut newer notes
        // early; releasing a single zone would split a stereo pair.
        let idx = ch * 128 + key as usize;
        let positions = &self.key_voices[idx];
        if !positions.is_empty() {
            let mut note_id: Option<u64> = None;
            for &pos in positions {
                let Some(v) = self.voices.get(pos) else {
                    continue;
                };
                if v.released || v.release_at != u64::MAX {
                    continue;
                }
                // First not-yet-releasing note wins; release all of its
                // zone voices together.
                note_id = Some(v.note_id);
                break;
            }
            if let Some(nid) = note_id {
                let mut released_any = false;
                for &pos in positions {
                    if let Some(v) = self.voices.get_mut(pos)
                        && v.note_id == nid
                        && !damper
                    {
                        v.release_at = at;
                        released_any = true;
                    }
                    // When the damper is down, the voice stays sustained
                    // until the damper is lifted (release_at stays MAX).
                }
                if released_any {
                    // The whole note group is now releasing; decrement the
                    // active-note count (never below 0 - trims may have
                    // stale-counted).
                    let slot = &mut self.active_notes[ch * 128 + key as usize];
                    *slot = slot.saturating_sub(1);
                }
            }
        }
        Ok(())
    }

    fn spawn_voices(&mut self, ch: usize, key: u8, vel: u8, at: u64) -> Result<(), SynthError> {
        // The per-key note-on budget is enforced in `handle_event` before
        // this call (see `MidiEvent::NoteOn`), so this path only runs for
        // notes that may actually be heard.
        let sf = self.sf.as_ref().ok_or_else(|| {
            SynthError::Config("no soundfont loaded; call load_soundfont first".into())
        })?;
        let pitch_mult = self.channels[ch].pitch_multiplier;
        // Envelope-modifier CC values also shape the template (attack/release
        // re-parameterization happens at build time).
        let env_attack = self.channels[ch].env_attack;
        let env_release = self.channels[ch].env_release;
        let tmpl_key = (
            key,
            vel,
            ch as u8,
            pitch_mult.to_bits(),
            env_attack.unwrap_or(0xFF),
            env_release.unwrap_or(0xFF),
        );
        // Build (or reuse) the voices for this note. Template hits are the
        // hot path on black-MIDI note storms: identical notes repeat
        // thousands of times per block, and cloning skips the zone lookup +
        // envelope computation entirely. Templates are immutable; the
        // per-note fields (id, note_id, start_at, state, release) are reset
        // on clone below.
        if let Some(tmpls) = self.voice_templates.get(&tmpl_key).cloned() {
            if tmpls.is_empty() {
                return Ok(());
            }
            self.note_counter += 1;
            let note_id = self.note_counter;
            for t in &tmpls {
                let mut v = t.clone();
                v.id = self.voice_id_counter;
                self.voice_id_counter += 1;
                v.note_id = note_id;
                v.start_at = at;
                v.state = VoiceState::default();
                v.release_at = u64::MAX;
                v.released = false;
                v.sample_offset_r = 0;
                v.spawn_frame = self.global_frame;
                let pos = self.voices.len();
                self.voices.push(v);
                self.key_voices[ch * 128 + key as usize].push_back(pos);
            }
            let slot = &mut self.active_notes[ch * 128 + key as usize];
            *slot = slot.saturating_add(1);
            let limit = self.config.max_voices_per_key;
            if limit > 0 && self.key_voices[ch * 128 + key as usize].len() > limit * 2 {
                self.trim_key_voices(ch as u8, key, limit);
            }
            return Ok(());
        }
        let zone_ids = sf.zones_at(key, vel).to_vec();
        let mut built: Vec<Voice> = Vec::with_capacity(zone_ids.len());
        for zone_id in zone_ids {
            if let Some(v) = build_voice(
                sf,
                zone_id,
                key,
                vel,
                ch as u8,
                at,
                self.config.sample_rate,
                pitch_mult,
                env_attack,
                env_release,
                self.config.envelope_curves,
            ) {
                built.push(v);
            }
        }
        if built.is_empty() {
            return Ok(());
        }
        self.voice_templates.insert(tmpl_key, built.clone());
        self.note_counter += 1;
        let note_id = self.note_counter;
        for mut voice in built {
            voice.id = self.voice_id_counter;
            self.voice_id_counter += 1;
            voice.note_id = note_id;
            voice.spawn_frame = self.global_frame;
            let pos = self.voices.len();
            self.voices.push(voice);
            self.key_voices[ch * 128 + key as usize].push_back(pos);
        }
        // One more active note group for this key (release_key decrements).
        let slot = &mut self.active_notes[ch * 128 + key as usize];
        *slot = slot.saturating_add(1);
        // In-block light trim: when a key's list exceeds twice its cap,
        // compact it right away. Deferring the whole trim to the block end
        // lets the list grow to tens of thousands of voices on black-MIDI
        // note storms, which makes every in-block release_key scan O(10k).
        let limit = self.config.max_voices_per_key;
        if limit > 0 && self.key_voices[ch * 128 + key as usize].len() > limit * 2 {
            self.trim_key_voices(ch as u8, key, limit);
        }
        Ok(())
    }

    /// Rebuilds the per-key voice index after any mutation of `voices`
    /// (retain-based removal changes all positions).
    fn rebuild_key_voices(&mut self) {
        for q in self.key_voices.iter_mut() {
            q.clear();
        }
        for (i, v) in self.voices.iter().enumerate() {
            self.key_voices[v.channel as usize * 128 + v.key as usize].push_back(i);
        }
    }

    fn apply_cc(&mut self, ch: usize, controller: u8, value: u8) {
        let sr = self.config.sample_rate;
        let mut pitch_dirty = false;
        match controller {
            // CC7 (volume), CC11 (expression), CC10/CC8 (pan) are handled by
            // `handle_event` -> `defer_mix_cc` for frame-exact application;
            // they never reach this function.
            0x07 | 0x0B | 0x0A | 0x08 => {
                debug_assert!(false, "CC7/11/10/8 must go through defer_mix_cc");
                let _ = sr;
            }
            0x47 => {
                // Resonance (CC71): unused by the SF2 voice path in XSynth
                // (voice resonance comes from the soundfont), but tracked for
                // completeness.
                let _ = value;
            }
            // ---- RPN / NRPN selection (pitch-critical) ----
            0x64 => {
                // CC100: RPN MSB.
                let inner = match self.channels[ch].param {
                    ParamSel::Rpn(_, l) => l,
                    _ => 0,
                };
                self.channels[ch].param = ParamSel::Rpn(value, inner);
            }
            0x65 => {
                // CC101: RPN LSB.
                let inner = match self.channels[ch].param {
                    ParamSel::Rpn(m, _) => m,
                    _ => 0,
                };
                self.channels[ch].param = ParamSel::Rpn(inner, value);
            }
            0x62 => {
                // CC98: NRPN MSB.
                let inner = match self.channels[ch].param {
                    ParamSel::Nrpn(_, l) => l,
                    _ => 0,
                };
                self.channels[ch].param = ParamSel::Nrpn(value, inner);
            }
            0x63 => {
                // CC99: NRPN LSB.
                let inner = match self.channels[ch].param {
                    ParamSel::Nrpn(m, _) => m,
                    _ => 0,
                };
                self.channels[ch].param = ParamSel::Nrpn(inner, value);
            }
            0x06 => {
                // CC6: Data Entry MSB (RPN/NRPN).
                self.channels[ch].data_msb = value;
                self.channels[ch].apply_rpn_data();
                pitch_dirty = true;
            }
            0x26 => {
                // CC38: Data Entry LSB (RPN/NRPN).
                self.channels[ch].data_lsb = value;
                self.channels[ch].apply_rpn_data();
                pitch_dirty = true;
            }
            0x48 => {
                // Release time (CC72): modifies the release envelope stage.
                self.channels[ch].env_release = Some(value);
                for v in &mut self.voices {
                    if v.channel as usize == ch {
                        v.env_release = Some(value);
                        refresh_env_stages(v);
                    }
                }
            }
            0x49 => {
                // Attack time (CC73): modifies the attack envelope stage.
                self.channels[ch].env_attack = Some(value);
                for v in &mut self.voices {
                    if v.channel as usize == ch {
                        v.env_attack = Some(value);
                        refresh_env_stages(v);
                    }
                }
            }
            0x40 => {
                let was_damper = self.channels[ch].damper;
                let damper = value >= 64;
                self.channels[ch].damper = damper;
                // Releasing the damper frees all voices that were sustained.
                if was_damper && !damper {
                    for v in &mut self.voices {
                        if v.channel as usize == ch
                            && !v.released
                            && v.release_at == u64::MAX
                            && v.state.ended == 0
                        {
                            v.release_at = self.global_frame;
                        }
                    }
                }
            }
            0x79 => {
                // Reset all controllers.
                self.channels[ch] = ChannelState::new();
                pitch_dirty = true;
            }
            0x7B => {
                // All notes off.
                for v in &mut self.voices {
                    if v.channel as usize == ch {
                        v.release_at = self.global_frame;
                    }
                }
            }
            0x78 => {
                // All sounds off: kill immediately.
                self.voices.retain(|v| v.channel as usize != ch);
                self.rebuild_key_voices();
            }
            _ => {}
        }
        if pitch_dirty {
            self.propagate_channel_pitch(ch);
        }
    }

    /// Recomputes a channel's `pitch_multiplier` is already done in
    /// `ChannelState`; this pushes the new multiplier onto every *active*
    /// voice of the channel so a bend-sensitivity or tuning change takes
    /// effect on sounding notes immediately (otherwise only future note-ons
    /// would track it, and held notes would sit at the old pitch).
    fn propagate_channel_pitch(&mut self, ch: usize) {
        if let Some(sf) = self.sf.as_ref() {
            let mult = self.channels[ch].pitch_multiplier;
            for v in &mut self.voices {
                if v.channel as usize == ch {
                    let zone = sf.zone(v.zone_id);
                    v.speed = zone.speed_mult * mult;
                }
            }
        }
    }

    /// Trims one (channel, key) voice list to the `limit` loudest note
    /// groups, releasing the rest (they fade out via their release
    /// envelope instead of being hard-killed - a hard kill makes a
    /// sounding voice vanish in one block, an audible click at the
    /// polyphony cap). Whole notes are released, never split zones -
    /// mirroring XSynth's `pop_quietest_voice_group` + `fade_out_killing`.
    /// O(key voices), so it must only run when the key exceeds its cap,
    /// not per note-on.
    fn trim_key_voices(&mut self, ch: u8, key: u8, limit: usize) {
        let idx = ch as usize * 128 + key as usize;
        let positions: Vec<usize> = self.key_voices[idx].iter().copied().collect();
        // Group by note_id (spawn order keeps one note's zones adjacent);
        // ended voices are freed for free. Sort OLDEST first so a fresh
        // note-on always survives the trim (XSynth's steal semantics: at
        // high NPS the newest notes sound, the oldest fade out).
        let mut groups: Vec<(u64, u8, Vec<usize>)> = Vec::new();
        for &pos in &positions {
            let Some(v) = self.voices.get(pos) else {
                continue;
            };
            if v.state.ended != 0 || v.release_at != u64::MAX {
                continue;
            }
            match groups.last_mut() {
                Some((_, _, g)) if self.voices[g[0]].note_id == v.note_id => g.push(pos),
                _ => groups.push((v.spawn_frame, v.vel, vec![pos])),
            }
        }
        // `active` is voices, but limit is groups — convert: groups = ceil(active / avg_voices_per_group)
        // For per-key, limit is groups, so need_free groups = groups.len() - limit
        let need_free = groups.len().saturating_sub(limit);
        if need_free == 0 {
            return;
        }
        // xsynth 抢占最安静的力度组，而非最老的，与 VoiceBuffer::pop_quietest_voice_group 一致
        groups.sort_by_key(|&(_, vel, _)| vel);
        // For dense black MIDI (>20k), hard-kill is inaudible (dense mix
        // masks the 1-block click) but saves 1 block of fading voices
        // (20k * 32ms tail = 640k voice-blocks). Flame showed fading
        // accumulation is the 80k→70k leak.
        let hard_kill = self.voices.len() > 20000;
        for (freed, (_, _, g)) in groups.iter().enumerate() {
            if freed >= need_free {
                break;
            }
            for &pos in g {
                if let Some(v) = self.voices.get_mut(pos)
                    && v.release_at == u64::MAX
                    && v.state.ended == 0
                {
                    if hard_kill {
                        v.state.ended = 1;
                    } else {
                        v.release_at = self.global_frame;
                        v.released = true;
                        v.fade_out = true;
                    }
                }
            }
        }
        // Compact the key index: drop ended and fading entries so
        // per-event scans (release_key, further trims) stay bounded by the
        // cap. Fading voices remain in `voices` for GPU but are not per-key.
        let kept: VecDeque<usize> = positions
            .iter()
            .copied()
            .filter(|&pos| {
                self.voices
                    .get(pos)
                    .is_some_and(|v| v.state.ended == 0 && v.release_at == u64::MAX)
            })
            .collect();
        self.key_voices[idx] = kept;
        // Rebuild this key's active-note count exactly. Decrementing by the
        // killed group count would underflow: the trimmed groups include
        // already-released notes that were decremented at release time, and
        // a stale (too-low) count makes the release fast path skip live
        // notes, leaving them sustained forever.
        let mut live_notes: Vec<u64> = Vec::new();
        for &pos in &positions {
            if let Some(v) = self.voices.get(pos)
                && v.state.ended == 0
                && v.release_at == u64::MAX
                && !live_notes.contains(&v.note_id)
            {
                live_notes.push(v.note_id);
            }
        }
        self.active_notes[ch as usize * 128 + key as usize] = live_notes.len() as u8;
    }

    /// Ends every voice whose exclusive class has a newer note (the newest
    /// note of a class wins, mirroring XSynth). Runs once per block in
    /// `upload_voices` - the previous per-note-on scan was O(voices) per
    /// event and dominated black-MIDI peak blocks.
    ///
    /// Killed voices FADE OUT (1 ms, XSynth's `ReleaseType::Kill`) instead
    /// of being hard-ended: a hard `ended = 1` here makes a sounding voice
    /// vanish instantly, an audible click - and in black-MIDI exclusive
    /// storms (the same note retriggered thousands of times) that is a
    /// continuous crackle, the user's "800-1000 voices and it pops"
    /// symptom. The fade stage is the release, so the voice ends 1 ms later
    /// and the newest note still wins immediately.
    fn trim_exclusive(&mut self) {
        let mut newest: std::collections::HashMap<u8, u64> = std::collections::HashMap::new();
        for v in &self.voices {
            if let Some(c) = v.exclusive_class {
                newest
                    .entry(c)
                    .and_modify(|n| *n = (*n).max(v.note_id))
                    .or_insert(v.note_id);
            }
        }
        if newest.is_empty() {
            return;
        }
        for v in &mut self.voices {
            if let Some(c) = v.exclusive_class
                && v.state.ended == 0
                && v.release_at == u64::MAX
                && newest.get(&c).is_some_and(|&n| n != v.note_id)
            {
                // Fade out instead of hard-ending (see the doc above).
                v.release_at = self.global_frame;
                v.released = true;
                v.fade_out = true;
            }
        }
    }

    fn upload_voices(&mut self, base: u64) -> Result<(), SynthError> {
        // The per-key note-on budget is per block.
        self.spawn_budget.fill(0);
        // Exclusive classes resolved once per block.
        self.trim_exclusive();
        // Rebuild the active-note counts exactly (trims/exclusive kills may
        // have left the in-block counters stale). After trim_exclusive, so
        // killed classes are not counted.
        self.active_notes.fill(0);
        for v in &self.voices {
            if v.state.ended == 0 && v.release_at == u64::MAX {
                let slot = &mut self.active_notes[v.channel as usize * 128 + v.key as usize];
                *slot = slot.saturating_add(1);
            }
        }
        // Per-key polyphony trim, deferred from `spawn_voices`: ending voices
        // per note-on was O(key voices) per event (black-MIDI storms scan
        // the key for every one of thousands of notes per block); trimming
        // once per block is O(voices). Semantics mirror XSynth's
        // `pop_quietest_voice_group`: keep the `max_voices_per_key` loudest
        // note *groups* of each key (whole notes are killed, never split
        // zones), quietest first.
        let per_key_limit = self.config.max_voices_per_key;
        if per_key_limit > 0 {
            let keys: Vec<(u8, u8)> = self
                .key_voices
                .iter()
                .enumerate()
                .filter(|(_, positions)| {
                    if positions.is_empty() {
                        return false;
                    }
                    // Count distinct note groups (XSynth semantics), not voices
                    let mut groups = 0usize;
                    let mut last_nid: Option<u64> = None;
                    for &pos in positions.iter() {
                        if let Some(v) = self.voices.get(pos)
                            && Some(v.note_id) != last_nid
                        {
                            groups += 1;
                            last_nid = Some(v.note_id);
                            if groups > per_key_limit {
                                return true;
                            }
                        }
                    }
                    false
                })
                .map(|(idx, _)| ((idx / 128) as u8, (idx % 128) as u8))
                .collect();
            for (ch, key) in keys {
                self.trim_key_voices(ch, key, per_key_limit);
            }
            self.voices.retain(|v| v.state.ended == 0);
            self.rebuild_key_voices();
        }

        // Drop voices that ended (state refreshed by the previous readback)
        // and rebuild the per-key index before borrowing the GPU device.
        self.voices.retain(|v| v.state.ended == 0);
        // Global voice cap (once per block, not per note-on): the physical
        // GPU pool (`max_voices + max_voices/FADE_SLOTS_FRACTION`) is the
        // hard limit - a pathological MIDI (black-MIDI note storms) would
        // otherwise run the upload past the pool buffers and crash the
        // dispatch with a wgpu validation error. Release the quietest note
        // groups until we fit - cheap here because it runs once per block,
        // not once per event.
        //
        // Voices already fading (from an earlier trim, 1 ms = one block)
        // are ended outright - their output has decayed, so ending them is
        // inaudible - while fresh trims fade out instead of hard-killing
        // (a hard kill makes a sounding voice vanish in one block, an
        // audible click/crackle).
        if self.config.max_voices != 0 {
            let pool = self.config.max_voices + self.config.max_voices / FADE_SLOTS_FRACTION;
            if self.voices.len() > pool {
                let over = self.voices.len() - pool;
                // Group voices by note in one O(n) pass (spawn order keeps one
                // note's zones adjacent), then release whole quietest groups
                // until the cap fits. The previous implementation re-scanned
                // the whole voice list per group (O(over x n)) - hundreds of ms
                // per block on black-MIDI note storms at the pool cap.
                let mut groups: Vec<(u64, u8, u64, Vec<usize>)> = Vec::new();
                for (i, v) in self.voices.iter().enumerate() {
                    match groups.last_mut() {
                        Some((_, _, note, _)) if *note == v.note_id => {}
                        _ => groups.push((v.spawn_frame, v.vel, v.note_id, Vec::new())),
                    }
                    groups.last_mut().unwrap().3.push(i);
                }
                // Oldest first: a freshly-spawned note must always sound, even
                // at extreme NPS (XSynth's steal semantics).
                groups.sort_by_key(|&(spawn, vel, _, _)| (spawn, vel));
                let fade_slots = self.config.max_voices / FADE_SLOTS_FRACTION;
                let mut fade_count = self.voices.iter().filter(|v| v.fade_out).count();
                let mut freed = 0usize;
                for (_, _, _, positions) in &groups {
                    if freed >= over {
                        break;
                    }
                    for &i in positions {
                        let v = &mut self.voices[i];
                        if v.release_at == u64::MAX {
                            if fade_count < fade_slots {
                                // Fade out instead of hard-killing: a hard kill
                                // makes a sounding voice vanish in one block,
                                // an audible click/crackle. A 1 ms linear fade
                                // (XSynth's `ReleaseType::Kill`) keeps the
                                // output continuous and the voice ends right
                                // after, so the pool does not accumulate tails.
                                v.release_at = self.global_frame;
                                v.released = true;
                                v.fade_out = true;
                                fade_count += 1;
                            } else {
                                // The fade slots are full (sustained overload):
                                // end the voice now. It is the OLDEST survivor
                                // (the sort above), so its output is already
                                // decaying - inaudible.
                                v.state.ended = 1;
                            }
                        } else {
                            // Already fading: end it now (output has decayed).
                            v.state.ended = 1;
                        }
                        freed += 1;
                    }
                }
                // NOTE: no retain here - the released voices stay in the pool
                // until their release envelope ends (then the GPU marks them
                // `ended` and the next block's readback prunes them).
            }
        }

        // Upload-capacity fallback: the physical pool buffers cannot hold
        // more than `pool` voices, and fading voices legitimately keep
        // occupying slots until their 1 ms fade completes. If the total
        // (active + fading) still exceeds the pool, end fading voices -
        // their output has already decayed, so this is inaudible. In the
        // pathological case where even the active voices alone exceed the
        // pool, end those too (order is preserved for the id-based state
        // resume).
        // Disabled in unlimited mode: buffers grow instead of trimming.
        if self.config.max_voices != 0 {
            // Measure against the voices still *alive* (not already marked
            // ended by the cap above). The first cap may have ended a large
            // fraction of the overflow, so the raw `voices.len()` would still
            // report the full pre-trim count and `kill` would be recomputed
            // from it - ending the survivors a second time and wiping the
            // entire pool at extreme polyphony (the "silence at high note
            // count" bug: a black-MIDI storm spawns far more voices in one
            // block than the pool, and the double-kill left zero voices).
            let alive = self.voices.iter().filter(|v| v.state.ended == 0).count();
            let pool = self.config.max_voices + self.config.max_voices / FADE_SLOTS_FRACTION;
            if alive > pool {
                let mut kill = alive - pool;
                for v in self.voices.iter_mut() {
                    if v.fade_out && kill > 0 {
                        v.state.ended = 1;
                        kill -= 1;
                    }
                }
                // Pathological: active voices alone exceed the pool. Kill
                // the OLDEST (spawn_frame), keeping fresh notes sounding.
                if kill > 0 {
                    let mut idx: Vec<usize> = (0..self.voices.len())
                        .filter(|&i| self.voices[i].state.ended == 0)
                        .collect();
                    idx.sort_by_key(|&i| (self.voices[i].spawn_frame, self.voices[i].vel));
                    for i in idx {
                        if kill > 0 {
                            self.voices[i].state.ended = 1;
                            kill -= 1;
                        }
                    }
                }
            }
        }
        self.voices.retain(|v| v.state.ended == 0);

        self.rebuild_key_voices();

        let n = self.voices.len();
        // Reuse the per-block upload buffers: `resize` keeps the allocation,
        // so a cap-sized pool does not re-allocate ~1.5 MB every block.
        self.upload_params.resize(n.max(1), VoiceParams::zeroed());
        self.upload_states.resize(n.max(1), VoiceState::zeroed());
        // Pre-compute per-voice env stage counts and prefix sums so the
        // env upload can be parallelized (each voice knows its base).
        let env_counts: Vec<u32> = self
            .voices
            .iter()
            .map(|v| {
                if v.fade_out {
                    1
                } else {
                    v.env_stages.len() as u32
                }
            })
            .collect();
        let mut env_bases: Vec<u32> = Vec::with_capacity(n);
        let mut total_env: usize = 0;
        for &c in &env_counts {
            env_bases.push(total_env as u32);
            total_env += c as usize;
        }
        self.upload_env_stages
            .resize(total_env.max(1), EnvStageGpu::zeroed());
        self.upload_chans.resize(n.max(1), 0);

        // Snapshot data needed for the parallel phase to avoid borrowing
        // `self` inside the closure.
        let sample_offsets = &self.sample_offsets;
        let prev_ids = self.prev_voice_ids.clone();
        let new_ids: Vec<u32> = self.voices.iter().map(|v| v.id).collect();
        let last_states = self.last_states.clone();
        let st_count = last_states
            .as_ref()
            .map_or(0, |st| st.len() / VoiceState::SIZE);
        let base_frame = base;
        let interp = self.config.interpolation;
        let sr = self.config.sample_rate;

        // Parallel path for large voice counts (black MIDI peaks). For small
        // n the rayon overhead outweighs the benefit, so keep the sequential
        // fast path.
        if n > 2048 {
            // Pre-fetch sample offsets and update per-voice `sample_offset_r`
            // in a first pass (hash lookups are not thread-safe for mutation,
            // so do them sequentially but cheap). Also fill env stages
            // sequentially (variable-length slices are not easily parallelized
            // without disjoint borrow issues).
            let mut sample_offs: Vec<(u32, u32)> = Vec::with_capacity(n);
            for (i, v) in self.voices.iter_mut().enumerate() {
                let off = sample_offsets
                    .get(&v.sample_id)
                    .map(|(o, _)| *o)
                    .unwrap_or(0);
                let off_r = sample_offsets
                    .get(&v.sample_id_r)
                    .map(|(o, _)| *o)
                    .unwrap_or(off);
                v.sample_offset_r = off_r;
                sample_offs.push((off, off_r));
                // Fill env stages for this voice (sequential, cheap).
                let env_base = env_bases[i] as usize;
                let env_count = env_counts[i] as usize;
                let slice = &mut self.upload_env_stages[env_base..env_base + env_count];
                if v.fade_out {
                    slice[0] = EnvStageGpu {
                        kind: 0,
                        target_val: 0.0,
                        duration: (sr / 1000).max(1),
                    };
                } else {
                    for (j, s) in v.env_stages.iter().enumerate() {
                        slice[j] = EnvStageGpu {
                            kind: s.kind,
                            target_val: s.target,
                            duration: s.duration,
                        };
                    }
                }
            }
            // Parallel generation of params/chans/states via owned Vecs to avoid
            // borrow checker issues with &mut self in rayon closures. The
            // disjoint slices are filled sequentially after the parallel map.
            let voices_ref = &self.voices;
            let prev_ids_ref = &prev_ids;
            let last_states_ref = &last_states;
            let env_bases_ref = &env_bases;
            let sample_offs_ref = &sample_offs;
            let ((params_vec, chans_vec), states_vec) = rayon::join(
                || {
                    rayon::join(
                        || {
                            (0..n)
                                .into_par_iter()
                                .map(|i| {
                                    let v = &voices_ref[i];
                                    let (off, off_r) = sample_offs_ref[i];
                                    let env_base = env_bases_ref[i];
                                    let mut p =
                                        v.gpu_params(off, off_r, env_base, base_frame, interp);
                                    if v.fade_out {
                                        p.env_count = 1;
                                        p.release_idx = 0;
                                        p.finished_idx = 1;
                                    }
                                    p
                                })
                                .collect::<Vec<_>>()
                        },
                        || {
                            (0..n)
                                .into_par_iter()
                                .map(|i| {
                                    let v = &voices_ref[i];
                                    v.channel as u32
                                        | ((if v.released || v.release_at != u64::MAX {
                                            1u32
                                        } else {
                                            0u32
                                        }) << 7)
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                },
                || {
                    (0..n)
                        .into_par_iter()
                        .map(|i| {
                            let v = &voices_ref[i];
                            let resumed = if v.state.ended != 0 {
                                None
                            } else {
                                match prev_ids_ref.binary_search(&v.id) {
                                    Ok(k) if k < st_count => {
                                        if let Some(buf) = last_states_ref.as_ref() {
                                            let off = k * VoiceState::SIZE;
                                            Some(*bytemuck::from_bytes::<VoiceState>(
                                                &buf[off..off + VoiceState::SIZE],
                                            ))
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                }
                            };
                            if v.state.ended != 0 {
                                v.state
                            } else {
                                resumed.unwrap_or(v.state)
                            }
                        })
                        .collect::<Vec<_>>()
                },
            );
            self.upload_params[..n].copy_from_slice(&params_vec);
            self.upload_chans[..n].copy_from_slice(&chans_vec);
            self.upload_states[..n].copy_from_slice(&states_vec);
        } else {
            let params = &mut self.upload_params;
            let states = &mut self.upload_states;
            let chans = &mut self.upload_chans;
            let env_stages = &mut self.upload_env_stages;
            let prev_ids_ref = &prev_ids;
            let mut k = 0usize;
            for (i, v) in self.voices.iter_mut().enumerate() {
                let sample_offset = sample_offsets
                    .get(&v.sample_id)
                    .map(|(off, _)| *off)
                    .unwrap_or(0);
                let sample_offset_r = sample_offsets
                    .get(&v.sample_id_r)
                    .map(|(off, _)| *off)
                    .unwrap_or(sample_offset);
                v.sample_offset_r = sample_offset_r;
                let env_base = env_bases[i];
                let env_count = env_counts[i];
                let slice = &mut env_stages[env_base as usize..(env_base + env_count) as usize];
                if v.fade_out {
                    slice[0] = EnvStageGpu {
                        kind: 0,
                        target_val: 0.0,
                        duration: (sr / 1000).max(1),
                    };
                } else {
                    for (j, s) in v.env_stages.iter().enumerate() {
                        slice[j] = EnvStageGpu {
                            kind: s.kind,
                            target_val: s.target,
                            duration: s.duration,
                        };
                    }
                }
                let mut gp =
                    v.gpu_params(sample_offset, sample_offset_r, env_base, base_frame, interp);
                if v.fade_out {
                    gp.env_count = 1;
                    gp.release_idx = 0;
                    gp.finished_idx = 1;
                }
                params[i] = gp;
                while k < prev_ids_ref.len() && prev_ids_ref[k] < v.id {
                    k += 1;
                }
                let resumed = match last_states.as_ref() {
                    Some(st)
                        if k < prev_ids_ref.len() && prev_ids_ref[k] == v.id && k < st_count =>
                    {
                        let off = k * VoiceState::SIZE;
                        Some(*bytemuck::from_bytes::<VoiceState>(
                            &st[off..off + VoiceState::SIZE],
                        ))
                    }
                    _ => None,
                };
                states[i] = if v.state.ended != 0 {
                    v.state
                } else {
                    resumed.unwrap_or(v.state)
                };
                chans[i] = v.channel as u32
                    | ((if v.released || v.release_at != u64::MAX {
                        1u32
                    } else {
                        0u32
                    }) << 7);
            }
        }
        self.prev_voice_ids = new_ids;
        // (Upload is deferred to `dispatch` so all GPU work - staging copies,
        // render pass, readback copies - happens in ONE submit; separate
        // submits measured ~9ms each of fixed wgpu/Vulkan overhead.)
        self.active_voice_count = n as u32;
        Ok(())
    }
    /// Uploads up to `max_bytes` of the given samples (resampling first,
    /// smallest first so a small budget still completes several samples).
    /// Returns how many samples were uploaded; the rest stay pending for
    /// the next call. Used both by `upload_new_samples` (voice-driven) and
    /// `prefetch_samples` (lookahead-driven, realtime playback).
    fn upload_samples(&mut self, needed: &[usize], max_bytes: usize) -> Result<usize, SynthError> {
        let sf = match self.sf.as_mut() {
            Some(sf) => sf,
            None => return Ok(0),
        };
        if needed.is_empty() || max_bytes == 0 {
            return Ok(0);
        }

        let mut ids: Vec<(usize, usize)> = needed
            .iter()
            .map(|&id| (id, sf.sample_data(id).len() * 4))
            .collect();
        ids.sort_by_key(|&(_, bytes)| bytes);
        let mut budget = max_bytes;
        let mut todo: Vec<usize> = Vec::new();
        for (id, bytes) in ids {
            if budget < bytes {
                continue; // does not fit the remaining budget; try next time
            }
            todo.push(id);
            budget -= bytes;
        }
        if todo.is_empty() && !needed.is_empty() {
            // No sample fits the budget (large samples): upload the smallest
            // one anyway so progress is never zero - a single sample cannot
            // be split across blocks.
            let smallest = needed
                .iter()
                .min_by_key(|&&id| sf.sample_data(id).len())
                .copied()
                .unwrap();
            todo.push(smallest);
        }
        if todo.is_empty() {
            return Ok(0);
        }

        let device = &self.res.ctx.device;
        let queue = &self.res.ctx.queue;
        let rate = self.config.sample_rate;
        // Resampling is the dominant CPU cost for large soundfonts; run it
        // in parallel (each sample is independent), then upload sequentially.
        let resampled: Vec<(usize, Arc<[f32]>)> = todo
            .par_iter()
            .map(|&sample_id| {
                let data = sf.resample_read(sample_id, rate);
                (sample_id, data)
            })
            .collect();

        for (sample_id, data) in resampled {
            sf.cache_resampled(sample_id, rate, data.clone());
            let len = data.len() as u32;
            let offset = self.samples_next_offset;
            let grown = write_samples(
                &mut self.samples_chunks,
                device,
                queue,
                offset as u64 * 4,
                bytemuck::cast_slice(&data),
            )?;
            if grown {
                self.render_bg_dirty = true;
            }
            self.sample_offsets.insert(sample_id, (offset, len));
            self.samples_next_offset = offset + len;
        }
        Ok(todo.len())
    }

    /// Uploads every sample the current voices need (bounded only by the
    /// GPU buffer). Voice-driven path; see `upload_samples`.
    fn upload_new_samples(&mut self) -> Result<(), SynthError> {
        let mut needed: Vec<usize> = Vec::new();
        for v in &self.voices {
            if !self.sample_offsets.contains_key(&v.sample_id) {
                needed.push(v.sample_id);
            }
            if !self.sample_offsets.contains_key(&v.sample_id_r) {
                needed.push(v.sample_id_r);
            }
        }
        needed.sort_unstable();
        needed.dedup();
        self.upload_samples(&needed, usize::MAX)?;
        Ok(())
    }

    /// Realtime-playback lookahead: pre-uploads samples the event stream
    /// will use within the next ~2 seconds, in chunks of at most
    /// `max_bytes`, so the render thread never stalls on a multi-hundred-ms
    /// resample+upload inside a block (which empties the audio queue and
    /// crackles). Returns how many samples were uploaded this call.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] if a sample upload fails.
    pub fn prefetch_samples(&mut self, max_bytes: usize) -> Result<usize, SynthError> {
        let Some(sf) = self.sf.as_ref() else {
            return Ok(0);
        };
        let horizon = self.global_frame + self.config.sample_rate as u64 * 2;
        let mut needed: Vec<usize> = Vec::new();
        for ev in self.offline_events.iter().skip(self.offline_cursor) {
            if (ev.sample as u64) > horizon {
                break;
            }
            if let MidiEvent::NoteOn { key, vel } = ev.event() {
                for &zid in sf.zones_at(key, vel) {
                    let z = sf.zone(zid);
                    if !self.sample_offsets.contains_key(&z.sample_id) {
                        needed.push(z.sample_id);
                    }
                    if !self.sample_offsets.contains_key(&z.sample_id_r) {
                        needed.push(z.sample_id_r);
                    }
                }
            }
        }
        needed.sort_unstable();
        needed.dedup();
        self.upload_samples(&needed, max_bytes)
    }
    fn update_mix_params(&mut self, base: u64) -> Result<(), SynthError> {
        let queue = &self.res.ctx.queue;
        let block = self.config.block_size as u32;
        let end = base + block as u64;
        let sr = self.config.sample_rate;

        // Take this block's deferred controller events; keep the rest for
        // the blocks that follow.
        let mut in_block: Vec<(u64, u8, u8, u8)> = Vec::new();
        let mut rest: Vec<(u64, u8, u8, u8)> = Vec::new();
        for ev in std::mem::take(&mut self.pending_mix_events) {
            if ev.0 < end {
                in_block.push(ev);
            } else {
                rest.push(ev);
            }
        }
        self.pending_mix_events = rest;
        in_block.sort_by_key(|e| e.0);

        // Frame-exact controller curve: the mix kernel replays this block's
        // events against the block-start lerp states, so the output does not
        // depend on the block size or on how many events a block contains.
        let events: Vec<MixEvent> = in_block
            .iter()
            .map(|e| MixEvent {
                frame: (e.0 - base) as u32,
                channel: e.1 as u32,
                cc: e.2 as u32,
                value: e.3 as f32 / 128.0,
            })
            .collect();

        // Per-channel block-start states, then advance the CPU-side lerp
        // state machines through this block (all events + the block end) so
        // the next block starts from the right values.
        let mut starts: Vec<MixStart> = Vec::with_capacity(MIX_CHANNELS);
        for ch_idx in 0..MIX_CHANNELS {
            let st = &mut self.channels[ch_idx];
            starts.push(MixStart {
                vol: st.volume.current,
                vol_step: st.volume.step,
                vol_end: st.volume.end,
                expr: st.expression.current,
                expr_step: st.expression.step,
                expr_end: st.expression.end,
                pan: st.pan.current,
                pan_step: st.pan.step,
                pan_end: st.pan.end,
                _pad: [0.0; 3],
            });
            for ev in in_block.iter().filter(|e| e.1 as usize == ch_idx) {
                let (s, cc, value) = (ev.0, ev.2, ev.3);
                match cc {
                    0x07 => {
                        st.volume.advance_to(s);
                        st.volume.set_end(value as f32 / 128.0, sr);
                    }
                    0x0B => {
                        st.expression.advance_to(s);
                        st.expression.set_end(value as f32 / 128.0, sr);
                    }
                    0x0A | 0x08 => {
                        st.pan.advance_to(s);
                        st.pan.set_end(value as f32 / 128.0, sr);
                    }
                    _ => {}
                }
            }
            st.volume.advance_to(end);
            st.expression.advance_to(end);
            st.pan.advance_to(end);
        }

        let device = &self.res.ctx.device;
        if self
            .mix_events_buf
            .write(device, queue, 0, bytemuck::cast_slice(&events))?
        {
            self.mix_bg_dirty = true;
        }
        let params = MixParams {
            voice_count: self.active_voice_count,
            block_size: block,
            channel_count: MIX_CHANNELS as u32,
            event_count: events.len() as u32,
            lerp_len: sr as f32 * 0.01,
            _pad: [0.0; 3],
            starts: starts
                .try_into()
                .map_err(|_| SynthError::Gpu("channel count mismatch".into()))?,
        };
        if std::env::var("LUMINO_VOICEDUMP").is_ok() && base > 415_000 && base < 420_000 {
            let s = &self.channels[0];
            eprintln!(
                "[mix] base={base} ch0 vol={:.4} expr={:.4} pan={:.4}",
                s.volume.current, s.expression.current, s.pan.current
            );
        }
        queue.write_buffer(&self.mix_params_buf, 0, bytemuck::cast_slice(&[params]));
        Ok(())
    }

    fn rebuild_bind_groups(&mut self) {
        let device = &self.res.ctx.device;
        let mut entries: Vec<wgpu::BindGroupEntry> = Vec::with_capacity(13);
        entries.push(wgpu::BindGroupEntry {
            binding: 0,
            resource: self.params_buf.buffer().as_entire_binding(),
        });
        for (i, chunk) in self.samples_chunks.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: SAMPLES_CHUNK_BINDING_BASE + i as u32,
                resource: chunk.buffer().as_entire_binding(),
            });
        }
        entries.extend([
            wgpu::BindGroupEntry {
                binding: crate::gpu::SINC_BINDING,
                resource: self.sinc_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: crate::gpu::ENV_BINDING,
                resource: self.env_buf.buffer().as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: crate::gpu::STATES_BINDING,
                resource: self.states_buf.buffer().as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: crate::gpu::VOICE_OUT_BINDING,
                resource: self.voice_out_buf.buffer().as_entire_binding(),
            },
        ]);
        self.render_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render bind group"),
            layout: &self.res.render_layout,
            entries: &entries,
        }));
        self.mix_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mix bind group"),
            layout: &self.res.mix_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.voice_out_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.out_storage_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.voice_chans_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.mix_events_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.mix_params_buf.as_entire_binding(),
                },
            ],
        }));
        self.render_bg_dirty = false;
        self.mix_bg_dirty = false;
    }

    fn rebuild_mix_bind_group(&mut self) {
        let device = &self.res.ctx.device;
        self.mix_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mix bind group"),
            layout: &self.res.mix_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.voice_out_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.out_storage_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.voice_chans_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.mix_events_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.mix_params_buf.as_entire_binding(),
                },
            ],
        }));
        self.mix_bg_dirty = false;
    }

    #[allow(clippy::modulo_one)] // STATES_SYNC_EVERY is 1; the cadence is configurable
    fn dispatch(&mut self, _base: u64) -> Result<(), SynthError> {
        let mut voices = self.active_voice_count;
        let block = self.config.block_size as u32;

        // Physical ceiling: the voice output buffer cannot exceed the
        // device's maximum buffer size. For limited mode report a clear
        // error; for unlimited (black-MIDI) chunking would be required to
        // exceed this, but the limit is ~524k voices at block 512 (~131k at
        // 2048), far beyond any real black MIDI peak (observed ~80k), so a
        // trim-to-fit fallback is practically unlimited and keeps the code
        // simple. A full chunked dispatch is reserved for a future change if
        // a file truly needs >500k simultaneous voices.
        if (voices as u64) * (block as u64) * 8 > MAX_VOICE_OUT_BYTES {
            if self.config.max_voices == 0 {
                let max_batch = (MAX_VOICE_OUT_BYTES / (block as u64 * 8)) as u32;
                eprintln!(
                    "[warn] voices {voices} * block {block} exceeds device buffer ({} bytes), capping to {max_batch} (oldest trimmed)",
                    MAX_VOICE_OUT_BYTES
                );
                // Trim oldest voices down to max_batch (same steal semantics
                // as the global cap, but at the device limit).
                let over = (voices - max_batch) as usize;
                // Group by note and keep newest (oldest trimmed).
                let mut groups: Vec<(u64, u8, u64, Vec<usize>)> = Vec::new();
                for (i, v) in self.voices.iter().enumerate() {
                    match groups.last_mut() {
                        Some((_, _, nid, _)) if *nid == v.note_id => {}
                        _ => groups.push((v.spawn_frame, v.vel, v.note_id, Vec::new())),
                    }
                    groups.last_mut().unwrap().3.push(i);
                }
                groups.sort_by_key(|&(spawn, vel, _, _)| (spawn, vel));
                let mut freed = 0usize;
                for (_, _, _, positions) in &groups {
                    if freed >= over {
                        break;
                    }
                    for &i in positions {
                        let v = &mut self.voices[i];
                        if v.state.ended == 0 {
                            v.state.ended = 1;
                            freed += 1;
                            if freed >= over {
                                break;
                            }
                        }
                    }
                }
                self.voices.retain(|v| v.state.ended == 0);
                self.rebuild_key_voices();
                self.active_voice_count = max_batch;
                voices = max_batch;
            } else {
                return Err(SynthError::VoiceLimit(voices as usize));
            }
        }

        // Grow the per-voice buffers if the active voice count exceeds the
        // current pool (dense MIDI may hold tens of thousands of voices).
        // Growing replaces the backing buffers, so bind groups are rebuilt
        // right below.
        if self.voice_out_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (voices * block * 2 * 4) as u64,
        ) {
            self.render_bg_dirty = true;
            self.mix_bg_dirty = true;
        }
        if self.voice_chans_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (voices * 4) as u64,
        ) {
            self.render_bg_dirty = true;
            self.mix_bg_dirty = true;
        }
        // The per-voice GPU storage buffers (params/states/env) are written by
        // the staging belt below at `n` voices, but unlike `voice_out_buf` /
        // `voice_chans_buf` they were only ever allocated at the pool size and
        // never grown. If the voice cap leaves slightly more than `pool` voices
        // (it releases whole note groups, so the survivor count can overshoot
        // by a group or two), the belt write overruns the fixed buffer and
        // wgpu aborts the whole submission with a validation error. Grow them
        // here, the same way the output buffers are grown. All three are bound
        // by the render bind group, so a grow dirties it for the rebuild below.
        if self.params_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (std::mem::size_of::<VoiceParams>() * voices as usize) as u64,
        ) {
            self.render_bg_dirty = true;
        }
        if self.states_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (std::mem::size_of::<VoiceState>() * voices as usize) as u64,
        ) {
            self.render_bg_dirty = true;
        }
        if self.env_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (std::mem::size_of::<EnvStageGpu>() * self.upload_env_stages.len().max(1)) as u64,
        ) {
            self.render_bg_dirty = true;
        }
        if self.render_bg_dirty {
            self.rebuild_bind_groups();
        }
        if self.mix_bg_dirty {
            self.rebuild_mix_bind_group();
        }

        let device = &self.res.ctx.device;
        let queue = &self.res.ctx.queue;
        let block = self.config.block_size as u32;
        let voices = self.active_voice_count;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("lumino block encoder"),
        });

        // Voice parameter uploads, staged through the persistent belt into
        // the SAME submission as the compute passes (one submit per block).
        {
            let n = (self.active_voice_count as usize)
                .min(self.upload_params.len())
                .max(1);
            self.belt
                .write_buffer(
                    &mut encoder,
                    self.params_buf.buffer(),
                    0,
                    wgpu::BufferSize::new((std::mem::size_of::<VoiceParams>() * n) as u64).unwrap(),
                    device,
                )
                .copy_from_slice(bytemuck::cast_slice(&self.upload_params[..n]));
            self.belt
                .write_buffer(
                    &mut encoder,
                    self.states_buf.buffer(),
                    0,
                    wgpu::BufferSize::new((std::mem::size_of::<VoiceState>() * n) as u64).unwrap(),
                    device,
                )
                .copy_from_slice(bytemuck::cast_slice(&self.upload_states[..n]));
            if !self.upload_env_stages.is_empty() {
                self.belt
                    .write_buffer(
                        &mut encoder,
                        self.env_buf.buffer(),
                        0,
                        wgpu::BufferSize::new(
                            (std::mem::size_of::<EnvStageGpu>() * self.upload_env_stages.len())
                                as u64,
                        )
                        .unwrap(),
                        device,
                    )
                    .copy_from_slice(bytemuck::cast_slice(&self.upload_env_stages));
            }
            self.belt
                .write_buffer(
                    &mut encoder,
                    self.voice_chans_buf.buffer(),
                    0,
                    wgpu::BufferSize::new((4 * n) as u64).unwrap(),
                    device,
                )
                .copy_from_slice(bytemuck::cast_slice(&self.upload_chans[..n]));
            self.belt.finish();
        }

        let render_bg = self
            .render_bg
            .as_ref()
            .ok_or_else(|| SynthError::Gpu("render bind group missing".into()))?;
        let mix_bg = self
            .mix_bg
            .as_ref()
            .ok_or_else(|| SynthError::Gpu("mix bind group missing".into()))?;

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("render pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.res.render_pipeline);
            pass.set_bind_group(0, render_bg, &[]);
            // Each voice is split across RENDER_SEGMENTS threads (gid.y);
            // the shader fast-forwards to its segment start, so the GPU
            // parallelism is voices x segments.
            pass.dispatch_workgroups(voices.div_ceil(128).max(1), crate::gpu::RENDER_SEGMENTS, 1);
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("mix pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.res.mix_pipeline);
            pass.set_bind_group(0, mix_bg, &[]);
            pass.dispatch_workgroups(block.div_ceil(128).max(1), 1, 1);
        }

        // Readbacks. The voice states are only copied back every
        // STATES_SYNC_EVERY blocks (see `states_sync_counter`); the output
        // must come back every block.
        let cur = self.out_readback_cur;
        encoder.copy_buffer_to_buffer(
            &self.out_storage_buf,
            0,
            &self.out_readback[cur],
            0,
            (self.config.block_size * 2 * 4) as u64,
        );
        let states_cur = self.states_readback_cur;
        if self.states_sync_counter == 0 {
            let states_bytes = (VoiceState::SIZE * self.voices.len()) as u64;
            let grew = self.states_readback[states_cur].ensure(device, queue, states_bytes);
            if grew {
                // The readback buffer was replaced; nothing else references
                // it (it is mapped below by value), so no rebind is needed.
            }
            encoder.copy_buffer_to_buffer(
                self.states_buf.buffer(),
                0,
                self.states_readback[states_cur].buffer(),
                0,
                states_bytes,
            );
        }

        let idx = queue.submit(Some(encoder.finish()));

        // One-block pipeline: this submission's readback is consumed by the
        // next `render_block` (`collect_pending_readback`), so the GPU runs
        // this block while the CPU maps the PREVIOUS block back - the
        // per-block synchronous poll wait is gone. Record the exact slots
        // the copies landed in: they are read back next block, and silent
        // blocks that skip dispatching never shift this window.
        self.pending = Some(PendingReadback {
            idx,
            out_slot: cur,
            states_slot: states_cur,
        });
        // `STATES_SYNC_EVERY = 1` keeps the counter at 0 (every block maps
        // its states); the modulo is intentional and kept for the
        // configurable cadence.
        self.states_sync_counter = if STATES_SYNC_EVERY > 1 {
            (self.states_sync_counter + 1) % STATES_SYNC_EVERY
        } else {
            0
        };
        Ok(())
    }

    /// Polls the pending submission (dispatched by the previous
    /// `render_block`) and reads its audio + voice states back into
    /// `last_out`/`last_states`. Called at the start of every
    /// `render_block`, including silent fast-path blocks: the GPU has had a
    /// full block of CPU work (apply/upload) to finish the pending
    /// submission, so the poll returns immediately instead of stalling.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] if the poll or either map fails.
    fn collect_pending_readback(&mut self) -> Result<(), SynthError> {
        let Some(p) = self.pending.take() else {
            return Ok(());
        };
        let device = &self.res.ctx.device;

        // Map requests (callbacks fire inside the poll below).
        let (otx, orx) = std::sync::mpsc::channel();
        self.out_readback[p.out_slot]
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = otx.send(r.is_ok());
            });
        let (stx, srx) = std::sync::mpsc::channel();
        let states_rb = self.states_readback[p.states_slot].buffer().clone();
        states_rb
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = stx.send(r.is_ok());
            });

        // The GPU has already finished the pending submission (it had this
        // block's CPU work to run in); this wait does not stall.
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(p.idx),
                timeout: None,
            })
            .map_err(|e| SynthError::Gpu(format!("poll failed: {e:?}")))?;

        let out_ok = orx.recv().unwrap_or(false);
        if out_ok {
            let slice = self.out_readback[p.out_slot].slice(..);
            self.last_out = Some(slice.get_mapped_range().to_vec());
            self.out_readback[p.out_slot].unmap();
        } else {
            return Err(SynthError::Gpu("output readback map failed".into()));
        }
        let states_ok = srx.recv().unwrap_or(false);
        if states_ok {
            let rb = self.states_readback[p.states_slot].buffer();
            let mapped = rb.slice(..).get_mapped_range();
            let bytes = mapped.to_vec();
            drop(mapped);
            self.last_states = Some(bytes);
            rb.unmap();
        } else {
            return Err(SynthError::Gpu("states readback map failed".into()));
        }

        // Both readback buffers are free again; alternate the slots.
        self.out_readback_cur ^= 1;
        self.states_readback_cur ^= 1;
        // The poll confirmed the pending submission (and every earlier one)
        // finished, so every closed belt chunk is safe to reclaim. wgpu 27's
        // StagingBelt additionally guards reuse itself: recalled chunks are
        // re-mapped and only re-enter the free pool after the GPU releases
        // them, so a chunk can never be overwritten while in flight.
        self.belt.recall();
        Ok(())
    }

    fn readback(&mut self, out: &mut [f32]) -> Result<(), SynthError> {
        let Some(data) = self.last_out.take() else {
            // First block of a stream has no previous dispatch to collect
            // from: the pipeline owes the listener silence for that block
            // (block 0's audio arrives with block 1).
            out.fill(0.0);
            return Ok(());
        };
        let count = (data.len() / 4).min(out.len());
        out[..count].copy_from_slice(bytemuck::cast_slice(&data[..count * 4]));
        // Lookahead output limiter (skip with LUMINO_NO_LIMITER for GPU
        // output diagnostics).
        if std::env::var("LUMINO_NO_LIMITER").is_err() {
            self.apply_limiter(out);
        }
        Ok(())
    }
    /// Lookahead output peak limiter (see the `limiter_gain` field doc).
    ///
    /// The mix pass returns the raw voice sum (f32 holds it losslessly even
    /// at hundreds/thousands of voices). A limiter is needed because the sum
    /// routinely exceeds full scale at high polyphony (64 voices peak ~10x,
    /// 800+ voices hundreds of times), and a hard clip / per-sample soft clip
    /// flat-tops the waveform into square-wave distortion (verified
    /// empirically, see the module history).
    ///
    /// WHY LOOKAHEAD (learned the hard way from the previous block-peak
    /// limiter): a limiter whose gain follows the signal with a finite
    /// response time faces a contradiction. A FAST gain (0.5 ms attack)
    /// tracks the signal tightly but the gain change itself is a fast
    /// multiplicative modulation of the whole mix - at 800+ live voices,
    /// where the sum exceeds full scale almost constantly and the block peak
    /// swings with every note, this modulation is a CONTINUOUS CRACKLE
    /// (the "800-1000 voices and above it starts popping" report). A SLOW
    /// gain (the other escape) avoids the modulation but lets onsets
    /// overshoot far past full scale before the gain descends - which then
    /// needs a deep soft clip that flat-tops the waveform again.
    ///
    /// The lookahead design resolves the contradiction: the mix block is
    /// already fully computed in RAM when the limiter runs, so the gain at
    /// frame `i` can be derived from the PEAK OF THE NEXT 64 FRAMES (1 ms @
    /// 64 kHz) instead of the past. The gain is therefore fully settled
    /// BEFORE the loud samples arrive: no overshoot, no fast modulation
    /// during onsets. The output is the mix delayed by 1 ms (a fixed,
    /// inaudible latency - 1 ms is far below the ~10 ms humans perceive).
    /// The gain itself still moves slowly (0.5 ms attack / 80 ms release),
    /// so it never modulates the signal audibly.
    ///
    /// The block's last 64 frames cannot see the next block, so their
    /// lookahead window is truncated; if the next block opens louder than
    /// this block's tail, a brief (< 1 ms) overshoot passes through
    /// `soft_knee` (a slope-continuous ceiling, not a flat-top clip).
    ///
    /// Below the 0.98 threshold (gain at unity) the limiter reduces to a
    /// pure 1 ms delay - the waveform is preserved bit-exactly, just shifted.
    fn apply_limiter(&mut self, out: &mut [f32]) {
        let sr = self.config.sample_rate as f32;
        limit_block(out, &mut self.limiter_tail, &mut self.limiter_gain, sr);
    }

    /// Applies the read-back voice states to the CPU mirror.
    ///
    /// Runs right after `collect_pending_readback` and BEFORE
    /// `upload_voices`, so the mirror reflects the GPU state of the block
    /// just read back (the one the next dispatch resumes from). A voice
    /// that ended on the GPU gets `v.state.ended = 1` here, which lets
    /// `upload_voices` prune it this same block.
    ///
    /// Maps by voice id (`prev_voice_ids` records the last upload order):
    /// the list may have shrunk since, so positional lookup would apply a
    /// stale state (and miss `ended`) on the wrong voice.
    ///
    /// Does NOT consume `prev_voice_ids`: that list must stay aligned with
    /// `last_states` (same upload order) for `upload_voices`' resume
    /// matching, which runs on the very same read-back states.
    fn sync_voice_states(&mut self) {
        let Some(states) = self.last_states.as_ref() else {
            return;
        };
        let count = states.len() / VoiceState::SIZE;
        if count == 0 {
            return;
        }
        // `prev_voice_ids` and `self.voices` are both sorted by voice id
        // (monotonic counter, `retain` keeps order), so a two-pointer merge
        // maps states back onto the mirror in O(n) - no HashMap build.
        let ids = &self.prev_voice_ids;
        let mut k = 0usize;
        for v in self.voices.iter_mut() {
            while k < ids.len() && ids[k] < v.id {
                k += 1;
            }
            if k >= ids.len() || ids[k] != v.id || k >= count {
                continue;
            }
            let off = k * VoiceState::SIZE;
            let st: &VoiceState = bytemuck::from_bytes(&states[off..off + VoiceState::SIZE]);
            v.state = *st;
            if st.ended != 0 {
                v.released = true;
            }
        }
    }
}

/// Slope-continuous soft ceiling for the limiter's attack window (samples
/// that still exceed 0.98 while the gain descends).
///
/// `y(0.98) = 0.98` with derivative 1.0 (matches the linear region), then
/// smoothly approaches 1.0 as `|x| -> inf`. Unlike a hard `clamp`, it never
/// produces flat-topped square-wave harmonics; unlike a per-sample soft clip
/// applied to the WHOLE signal it only engages above 0.98, so normal-range
/// audio is untouched (and reference renders stay bit-identical).
fn soft_knee(v: f32) -> f32 {
    let x = v.abs();
    // Below the knee the signal passes through untouched - the limiter's
    // gain scaling already did its job there.
    if x <= 0.98 {
        return v;
    }
    // t in [0, inf); tanh(0)=0 with slope 1, so the knee is slope-continuous.
    let t = (x - 0.98) / 0.02;
    let y = 0.98 + 0.02 * t.tanh();
    y.copysign(v)
}

/// Writes resampled sample bytes across the fixed-size sample chunks,
/// splitting at chunk boundaries. Returns `true` when any chunk grew
/// (bind groups must be rebuilt).
///
/// A free function so the caller can borrow `sf` (soundfont) and
/// `samples_chunks` as disjoint fields of the engine.
fn write_samples(
    chunks: &mut [GrowableBuffer],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    byte_offset: u64,
    data: &[u8],
) -> Result<bool, SynthError> {
    let mut off = byte_offset;
    let mut remaining = data;
    let mut grown = false;
    while !remaining.is_empty() {
        let chunk = (off / SAMPLES_CHUNK_BYTES) as usize;
        let Some(buf) = chunks.get_mut(chunk) else {
            return Err(SynthError::Gpu(format!(
                "sample data exceeds the chunked samples buffer capacity \
                 ({} chunks of {} MiB)",
                SAMPLES_CHUNKS,
                SAMPLES_CHUNK_BYTES / (1024 * 1024)
            )));
        };
        let in_chunk = off % SAMPLES_CHUNK_BYTES;
        let take = ((SAMPLES_CHUNK_BYTES - in_chunk) as usize).min(remaining.len());
        grown |= buf.write(device, queue, in_chunk, &remaining[..take])?;
        off += take as u64;
        remaining = &remaining[take..];
    }
    Ok(grown)
}

/// A single-line `\r`-rewritten progress bar for offline rendering.
///
/// The bar shows the fraction of the render horizon that is complete. It is
/// a no-op when disabled (library callers), and rewrites one line on stderr
/// so long exports stay visibly alive without flooding the log.
struct ProgressBar {
    /// Total frame count the bar is measured against.
    total: u64,
    /// Progress bar width in characters.
    width: usize,
    /// Last reported percent, so the bar only repaints when it changes.
    last_pct: i32,
    /// Whether output is enabled at all.
    enabled: bool,
}

impl ProgressBar {
    fn new(total: u64, enabled: bool) -> Self {
        Self {
            total: total.max(1),
            width: 24,
            last_pct: -1,
            enabled,
        }
    }

    /// Advances the bar to `done` frames and repaints when the percent
    /// crossed a whole-number boundary.
    fn tick(&mut self, done: u64) {
        if !self.enabled {
            return;
        }
        let pct = ((done as f64 / self.total as f64) * 100.0) as i32;
        if pct <= self.last_pct {
            return;
        }
        self.last_pct = pct;
        self.paint(pct);
    }

    /// Ends the bar on its own line (100% or the last painted value).
    fn finish(&mut self) {
        if !self.enabled {
            return;
        }
        self.last_pct = 100;
        self.paint(100);
        eprintln!();
    }

    fn paint(&self, pct: i32) {
        let pct = pct.clamp(0, 100);
        let filled = (pct as usize * self.width) / 100;
        let bar: String = std::iter::repeat_n('=', filled)
            .chain(std::iter::repeat_n(' ', self.width - filled))
            .collect();
        eprint!("\r[render] [{}] {pct:3}%", bar);
    }
}
#[cfg(test)]
mod tests {
    use super::{ChannelState, ParamSel, limit_block};

    const LOOKAHEAD: usize = 256;

    #[test]
    fn limiter_kills_single_sample_spike() {
        let n = 512usize;
        let mut tail = vec![0.0f32; LOOKAHEAD * 2];
        let mut gain = 1.0f32;
        let mut out = vec![0.5f32; n * 2];
        // Single-sample +3 spike at frame 100; the limiter delays by
        // LOOKAHEAD so it is emitted at output frame 356.
        out[100 * 2] = 3.0;
        out[100 * 2 + 1] = 3.0;

        limit_block(&mut out, &mut tail, &mut gain, 64000.0);

        // The original forward-only window missed this spike entirely; the new
        // window (centred on the emitted sample) must attenuate it. soft_knee
        // caps the output at 1.0, so a surviving spike would blow past that.
        let max_abs = out.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        assert!(
            max_abs < 1.0,
            "single-sample spike not limited (max abs = {max_abs})"
        );
    }

    #[test]
    fn limiter_kills_end_of_block_spike_across_blocks() {
        let n = 512usize;
        let mut tail = vec![0.0f32; LOOKAHEAD * 2];
        let mut gain = 1.0f32;
        // Block 0 ends with a spike in its very last sample.
        let mut block0 = vec![0.5f32; n * 2];
        block0[(n - 1) * 2] = 3.0;
        block0[(n - 1) * 2 + 1] = 3.0;
        limit_block(&mut block0, &mut tail, &mut gain, 64000.0);

        // Block 1 is quiet; the spike now lives in 	ail and is emitted near
        // block 1's start, so the gain window must reach into the tail to see it.
        let mut block1 = vec![0.5f32; n * 2];
        limit_block(&mut block1, &mut tail, &mut gain, 64000.0);

        for (name, b) in [("block0", &block0), ("block1", &block1)] {
            let max_abs = b.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
            assert!(
                max_abs < 1.0,
                "{name}: end-of-block spike not limited (max abs = {max_abs})"
            );
        }
    }

    #[test]
    fn limiter_sanitizes_nonfinite() {
        let n = 512usize;
        let mut tail = vec![0.0f32; LOOKAHEAD * 2];
        let mut gain = 1.0f32;
        let mut out = vec![0.5f32; n * 2];
        out[50 * 2] = f32::NAN;
        out[50 * 2 + 1] = f32::INFINITY;

        limit_block(&mut out, &mut tail, &mut gain, 64000.0);

        assert!(out[50 * 2].is_finite(), "NaN was not sanitized");
        assert!(out[50 * 2 + 1].is_finite(), "Inf was not sanitized");
        let max_abs = out.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        assert!(
            max_abs < 1.0,
            "non-finite artifact leaked a pop (max abs = {max_abs})"
        );
    }

    #[test]
    fn rpn_pitch_bend_sensitivity_24() {
        let mut st = ChannelState::new();
        // RPN 0 (Pitch Bend Sensitivity) selected, then Data Entry = 24 semitones
        // -- exactly what right-example.mid sends via CC100/101/6.
        st.param = ParamSel::Rpn(0, 0);
        st.data_msb = 24;
        st.data_lsb = 0;
        st.apply_rpn_data();
        assert!(
            (st.bend_sensitivity - 24.0).abs() < 1e-3,
            "bend sensitivity = {} (expected 24)",
            st.bend_sensitivity
        );

        // Bend value 9000 (above center 8192) must now map to ~2.36 semitones,
        // not the old hardcoded 2-semitone value (~0.197 semitones). This is the
        // "severe detuning" bug: every bend was scaled ~12x too small.
        st.bend_value = 9000;
        st.recompute_pitch();
        let expected = 2.0f32.powf(((9000.0 - 8192.0) / 8192.0 * 24.0) / 12.0);
        assert!(
            (st.pitch_multiplier - expected).abs() < 1e-4,
            "mult = {} (expected {})",
            st.pitch_multiplier,
            expected
        );
        // Sanity: clearly shifted up, far beyond the old 2-semitone scaling.
        assert!(
            st.pitch_multiplier > 1.1,
            "bend not scaled by 24 semitones (mult = {})",
            st.pitch_multiplier
        );
    }

    #[test]
    fn rpn_default_sensitivity_is_2() {
        let st = ChannelState::new();
        assert!(
            (st.bend_sensitivity - 2.0).abs() < 1e-3,
            "default sensitivity must be 2 semitones (GM)"
        );
    }

    #[test]
    fn rpn_channel_tuning() {
        let mut st = ChannelState::new();
        // RPN 2 (coarse tuning) = +3 semitones (MSB 67, center 64).
        st.param = ParamSel::Rpn(0, 2);
        st.data_msb = 67;
        st.data_lsb = 0;
        st.apply_rpn_data();
        st.recompute_pitch();
        let expected = 2.0f32.powf(3.0 / 12.0);
        assert!(
            (st.pitch_multiplier - expected).abs() < 1e-4,
            "coarse mult = {} (expected {})",
            st.pitch_multiplier,
            expected
        );

        // RPN 1 (fine tuning) = +50 cents (14-bit: 8192 + 0.5*8192 = 12288).
        st.param = ParamSel::Rpn(0, 1);
        st.data_msb = (12288 >> 7) as u8;
        st.data_lsb = (12288 & 0x7f) as u8;
        st.apply_rpn_data();
        st.recompute_pitch();
        // Total tuning = +3 semitones + 50 cents.
        let expected2 = 2.0f32.powf((300.0 + 50.0) / 1200.0);
        assert!(
            (st.pitch_multiplier - expected2).abs() < 1e-4,
            "fine+coarse mult = {} (expected {})",
            st.pitch_multiplier,
            expected2
        );
    }
}
