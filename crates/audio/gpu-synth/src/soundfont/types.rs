use super::*;

/// A note-on computed zone: all parameters required to spawn one voice for a
/// specific `(key, velocity)` pair, mirroring XSynth's spawner parameters.
#[derive(Debug, Clone)]
pub struct Zone {
    /// Index into the sample list of the owning [`SoundFont`] of the (left/mono)
    /// sample data, stored at the soundfont's native sample rate.
    pub sample_id: usize,
    /// Index of the right-channel sample data (== `sample_id` for mono).
    pub sample_id_r: usize,
    /// Number of channels in the sample (1 = mono, 2 = stereo pair).
    pub channels: u32,
    /// Amplitude gain (velocity-modulated).
    pub volume: f32,
    /// Stereo balance in `0..=1` (`0.5` = center).
    pub pan: f32,
    /// Playback speed multiplier (pitch cents).
    pub speed_mult: f32,
    /// Low-pass cutoff in Hz, if the zone has one and effects are enabled.
    pub cutoff: Option<f32>,
    /// Filter resonance in dB.
    pub resonance_db: f32,
    /// Loop mode.
    pub loop_mode: LoopMode,
    /// Loop start (native sample rate domain).
    pub loop_start: u32,
    /// Loop end (native sample rate domain).
    pub loop_end: u32,
    /// Playback start offset (native sample rate domain).
    pub offset: u32,
    /// Sample end / stop position (native sample rate domain).
    pub sample_end: u32,
    /// Volume envelope descriptor (seconds / percent).
    pub envelope: EnvelopeDescriptor,
    /// Exclusive class (voices sharing a class kill each other).
    pub exclusive_class: Option<u8>,
    /// Native sample rate of `sample_id` in the stored data domain
    /// (SF2: the load-time/engine rate; SFZ: the file's native rate).
    pub native_rate: u32,
}

impl Zone {
    /// Converts the loop/offset/end positions into the resampled domain
    /// (`new_rate`), using the same `convert_sample_index` helper as XSynth.
    pub fn convert_positions(&self, new_rate: u32) -> ZonePositions {
        let convert = |idx: u32| -> u32 {
            xsynth_soundfonts::convert_sample_index(idx, self.native_rate, new_rate)
        };
        ZonePositions {
            offset: convert(self.offset),
            loop_start: convert(self.loop_start).min(convert(self.sample_end)),
            loop_end: convert(self.loop_end).min(convert(self.sample_end)),
            sample_end: convert(self.sample_end),
        }
    }
}

/// Resampled-domain position data for a zone.
#[derive(Debug, Clone, Copy)]
pub struct ZonePositions {
    /// Playback start offset.
    pub offset: u32,
    /// Loop start.
    pub loop_start: u32,
    /// Loop end.
    pub loop_end: u32,
    /// Sample end.
    pub sample_end: u32,
}

/// A parsed soundfont ready for synthesis.
///
/// # Example
///
/// ```no_run
/// use lumino_gpu_synth::SoundFont;
///
/// let sf = SoundFont::load("assets/test.sf2", 0, 0, true, 48_000).unwrap();
/// assert!(!sf.zones_at(60, 100).is_empty());
/// ```
#[derive(Debug)]
pub struct SoundFont {
    pub(super) bank: u16,
    pub(super) preset: u16,
    /// All sample arrays at their native rate (deduplicated by pointer).
    pub(super) samples: Vec<Arc<[f32]>>,
    /// Per-(key, velocity) zone lists (index = key * 128 + vel).
    pub(super) zone_matrix: Vec<Vec<u16>>,
    /// All zones (indexed by the matrix entries).
    pub(super) zones: Vec<Zone>,
    /// Dedup map: sample Arc pointer as u64 -> sample id.
    pub(super) sample_ids: HashMap<u64, usize>,
    /// Whether voice effects (cutoff filter) are enabled.
    pub use_effects: bool,
    /// Rate SF2 sample data was resampled to at load (engine rate); also the
    /// fallback for [`SoundFont::native_rate_for`].
    pub(super) sample_rate: u32,
    /// Resample cache: (sample id, target rate) -> resampled data.
    pub(super) resample_cache: HashMap<(usize, u32), Arc<[f32]>>,
}
