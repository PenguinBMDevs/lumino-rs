//! CPU-side voice model and GPU parameter assembly.

use xsynth_soundfonts::LoopMode;

use crate::config::InterpolationMode;
use crate::gpu::{EnvStageGpu, VoiceParams, VoiceState};
use crate::soundfont::{SoundFont, Zone, ZonePositions};
use crate::synth::dsp::{
    EnvStage, EnvelopeCurveConfig, EnvelopeDescriptor, biquad_lowpass_coeffs, modify_env_stages,
    resonance_to_q, to_gpu_stages,
};

/// A single active voice (one zone of one note).
///
/// `Clone` powers the voice-template cache (`GpuSynth::voice_templates`):
/// black-MIDI note storms spawn thousands of identical (key, vel, channel)
/// notes per block, and cloning a pre-built voice skips the zone lookup and
/// envelope-stage computation per note.
#[derive(Debug, Clone)]
pub struct Voice {
    /// Index of this voice in the engine's voice list (GPU voice id).
    pub id: u32,
    /// Spawn batch id: all zones of one note-on share it. Voice stealing
    /// kills whole batches (like XSynth's voice groups) so a stereo pair
    /// is never split.
    pub note_id: u64,
    /// MIDI key.
    pub key: u8,
    /// MIDI velocity (used to steal the quietest voice, like XSynth).
    pub vel: u8,
    /// MIDI channel.
    pub channel: u8,
    /// Zone id in the soundfont.
    pub zone_id: u16,
    /// CPU mirror of the GPU voice state.
    pub state: VoiceState,
    /// Absolute global frame at which release starts (`u64::MAX` = none).
    pub release_at: u64,
    /// Whether the voice has been released already.
    pub released: bool,
    /// Exclusive class, if any.
    pub exclusive_class: Option<u8>,
    /// Absolute global frame at which the voice becomes audible.
    pub start_at: u64,
    /// Absolute global frame at which the voice was spawned. Polyphony
    /// trims kill the OLDEST voices first (XSynth's steal semantics: a
    /// freshly-spawned note must always sound), so at high NPS the newest
    /// notes survive instead of being trimmed into silence.
    pub spawn_frame: u64,
    /// Resampled-domain positions.
    pub positions: ZonePositions,
    /// Resampled sample length.
    pub sample_len: u32,
    /// Sample id in the soundfont (left channel).
    pub sample_id: usize,
    /// Sample id in the soundfont (right channel; == `sample_id` for mono).
    pub sample_id_r: usize,
    /// Buffer offset of the right channel sample data (filled by the engine).
    pub sample_offset_r: u32,
    /// Playback speed (cents factor * channel pitch multiplier).
    pub speed: f32,
    /// Static amplitude.
    pub amp: f32,
    /// Left/right zone pan gains.
    pub pan_l: f32,
    pub pan_r: f32,
    /// Loop mode.
    pub loop_mode: LoopMode,
    /// Filter coefficients (b0..a2), if the voice is filtered.
    pub filter: Option<[f32; 5]>,
    /// Envelope stages (pre-collapsed).
    pub env_stages: Vec<EnvStage>,
    /// Original envelope descriptor (for CC72/73 re-parameterization).
    pub envelope_desc: EnvelopeDescriptor,
    /// Sample rate used to parameterize the envelope.
    pub envelope_rate: u32,
    /// Envelope curve configuration used to build the stages.
    pub envelope_curves: EnvelopeCurveConfig,
    /// CC73 value that modified the attack (None = not modified).
    pub env_attack: Option<u8>,
    /// CC72 value that modified the release (None = not modified).
    pub env_release: Option<u8>,
    /// Index of the release stage within `env_stages`.
    pub release_idx: u32,
    /// Index of the terminal stage.
    pub finished_idx: u32,
    /// Whether the voice is being trimmed for polyphony and must fade out
    /// fast (XSynth's `ReleaseType::Kill`: 1 ms linear fade to zero) instead
    /// of using its normal release envelope. A hard kill makes a sounding
    /// voice vanish in one block - an audible click at the polyphony cap.
    pub fade_out: bool,
    /// Number of sample channels (1 = mono, 2 = stereo pair).
    pub channels: u32,
}

impl Voice {
    /// Assembles the GPU parameters for this voice.
    pub fn gpu_params(
        &self,
        sample_offset: u32,
        sample_offset_r: u32,
        env_base: u32,
        base_frame: u64,
        interp: InterpolationMode,
    ) -> VoiceParams {
        let pos = self.positions;
        VoiceParams {
            is_active: 1,
            sample_offset,
            sample_offset_r,
            sample_len: self.sample_len,
            offset: pos.offset,
            sample_end: pos.sample_end.saturating_sub(pos.offset),
            loop_mode: match self.loop_mode {
                LoopMode::NoLoop | LoopMode::OneShot => 0,
                LoopMode::LoopContinuous => 1,
                LoopMode::LoopSustain => 2,
            },
            loop_start: pos.loop_start,
            loop_end: pos.loop_end,
            speed: self.speed,
            amp: self.amp,
            pan_l: self.pan_l,
            pan_r: self.pan_r,
            filter_on: self.filter.is_some() as u32,
            b0: self.filter.map_or(0.0, |c| c[0]),
            b1: self.filter.map_or(0.0, |c| c[1]),
            b2: self.filter.map_or(0.0, |c| c[2]),
            a1: self.filter.map_or(0.0, |c| c[3]),
            a2: self.filter.map_or(0.0, |c| c[4]),
            env_base,
            env_count: self.env_stages.len() as u32,
            release_idx: self.release_idx,
            finished_idx: self.finished_idx,
            release_at: if self.released || self.release_at == u64::MAX {
                VoiceParams::RELEASE_AT_NONE
            } else {
                (self.release_at & 0xFFFF_FFFF) as u32
            },
            base_frame: (base_frame & 0xFFFF_FFFF) as u32,
            interp: match interp {
                InterpolationMode::Linear => 0,
                InterpolationMode::Point64Sinc => 1,
            },
            channels: self.channels,
            start_at: (self.start_at & 0xFFFF_FFFF) as u32,
            channel: self.channel as u32,
        }
    }

    /// Builds the env stage GPU entries for this voice.
    pub fn gpu_env_stages(&self) -> Vec<EnvStageGpu> {
        self.env_stages
            .iter()
            .map(|s| EnvStageGpu {
                kind: s.kind,
                target_val: s.target,
                duration: s.duration,
            })
            .collect()
    }
}

/// Builds a [`Voice`] from a zone for `(key, vel)` on `channel`.
///
/// Returns `None` if the zone's sample cannot be used.
#[allow(clippy::too_many_arguments)]
pub fn build_voice(
    sf: &SoundFont,
    zone_id: u16,
    key: u8,
    vel: u8,
    channel: u8,
    start_at: u64,
    sample_rate: u32,
    pitch_multiplier: f32,
    env_attack: Option<u8>,
    env_release: Option<u8>,
    envelope_curves: EnvelopeCurveConfig,
) -> Option<Voice> {
    let zone: &Zone = sf.zone(zone_id);
    let positions = zone.convert_positions(sample_rate);

    let filter = zone
        .cutoff
        .map(|freq| biquad_lowpass_coeffs(freq, sample_rate, resonance_to_q(zone.resonance_db)));

    // XSynth's envelope curves; CC72/73 modifications follow
    // `get_modified_envelope`.
    let envelope_desc = zone.envelope;
    let mut gpu_env = to_gpu_stages(&envelope_desc, sample_rate, envelope_curves);
    modify_env_stages(&mut gpu_env, sample_rate, env_attack, env_release);
    let release_idx = gpu_env
        .release_idx
        .unwrap_or(gpu_env.stages.len().saturating_sub(1)) as u32;
    let finished_idx = (gpu_env.stages.len().saturating_sub(1)) as u32;
    let env_stages = gpu_env.stages;

    let pan = zone.pan * std::f32::consts::FRAC_PI_2;
    let pan_l = (pan.cos() * 1.42).min(1.0);
    let pan_r = (pan.sin() * 1.42).min(1.0);

    Some(Voice {
        id: 0,
        note_id: 0,
        key,
        vel,
        channel,
        zone_id,
        // New voices start their envelope at the SF2 envelope START value
        // (XSynth: `EnvelopeParameters.start = start_percent`, the delay
        // stage's start/target; the attack stage ramps FROM that value to
        // 1.0). Starting from the first stage's TARGET instead would make
        // every new voice begin at full amplitude - with thousands of
        // simultaneous note-ons (black-MIDI) that onset step is an audible
        // click/crackle.
        state: VoiceState {
            env_from: envelope_desc.start_percent,
            ..Default::default()
        },
        release_at: u64::MAX,
        released: false,
        exclusive_class: zone.exclusive_class,
        start_at,
        spawn_frame: 0, // set by the engine when the voice is spawned
        positions,
        sample_len: 0,
        sample_id: zone.sample_id,
        sample_id_r: zone.sample_id_r,
        sample_offset_r: 0,
        speed: zone.speed_mult * pitch_multiplier,
        amp: zone.volume,
        pan_l,
        pan_r,
        loop_mode: zone.loop_mode,
        filter,
        env_stages,
        envelope_desc,
        envelope_rate: sample_rate,
        envelope_curves,
        env_attack,
        env_release,
        release_idx,
        finished_idx,
        fade_out: false,
        channels: zone.channels,
    })
}

/// Re-parameterizes a voice's envelope stages after a CC72/73 change.
pub fn refresh_env_stages(v: &mut Voice) {
    let mut gpu_env = to_gpu_stages(&v.envelope_desc, v.envelope_rate, v.envelope_curves);
    modify_env_stages(&mut gpu_env, v.envelope_rate, v.env_attack, v.env_release);
    v.env_stages = gpu_env.stages;
    v.release_idx = gpu_env
        .release_idx
        .unwrap_or(v.env_stages.len().saturating_sub(1)) as u32;
    v.finished_idx = (v.env_stages.len().saturating_sub(1)) as u32;
}
