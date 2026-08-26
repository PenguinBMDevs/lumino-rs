//! Soundfont loading and zone pre-computation.
//!
//! SF2 files are parsed with `xsynth-soundfonts` (the same parser used by the
//! XSynth engine), which means the region data - key/velocity ranges, root
//! keys, loop points, volume envelopes, filter cutoffs and baked note-on
//! modulators - is identical to what XSynth consumes.
//!
//! Sample data is kept at its native rate and **resampled lazily** into the
//! output sample rate the first time it is needed (using the exact same
//! `rubato` sinc resampler as XSynth), so a 400 MB soundfont costs nothing
//! until its samples are actually played.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use xsynth_soundfonts::LoopMode;
use xsynth_soundfonts::sf2::load_soundfont;

use crate::error::SoundFontError;
use crate::synth::dsp::{EnvelopeDescriptor, cents_factor};

/// A note-on computed zone: all parameters required to spawn one voice for a
/// specific `(key, velocity)` pair, mirroring XSynth's spawner parameters.
#[derive(Debug, Clone)]
pub struct Zone {
    /// Index into [`SoundFont::samples`] of the (left/mono) sample data,
    /// stored at the soundfont's native sample rate.
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
    /// Native sample rate of `sample_id`.
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
/// ```
/// use lumino_gpu_synth::SoundFont;
///
/// let sf = SoundFont::load("assets/test.sf2", 0, 0, true).unwrap();
/// assert!(!sf.zones_at(60, 100).is_empty());
/// ```
#[derive(Debug)]
pub struct SoundFont {
    bank: u16,
    preset: u16,
    /// All sample arrays at their native rate (deduplicated by pointer).
    samples: Vec<Arc<[f32]>>,
    /// Per-(key, velocity) zone lists (index = key * 128 + vel).
    zone_matrix: Vec<Vec<u16>>,
    /// All zones (indexed by the matrix entries).
    zones: Vec<Zone>,
    /// Dedup map: sample Arc pointer as u64 -> sample id.
    sample_ids: HashMap<u64, usize>,
    /// Whether voice effects (cutoff filter) are enabled.
    pub use_effects: bool,
    /// Resample cache: (sample id, target rate) -> resampled data.
    resample_cache: HashMap<(usize, u32), Arc<[f32]>>,
}

impl SoundFont {
    /// Loads an SF2 file and keeps only the requested `bank`/`preset`.
    ///
    /// Samples are *not* resampled here; they are resampled lazily on first
    /// use by the engine (see [`SoundFont::resample`]).
    ///
    /// # Errors
    ///
    /// Returns [`SoundFontError::Parse`] when the file is not a valid SF2,
    /// and [`SoundFontError::MissingPreset`] when the bank/preset does not
    /// exist in the file.
    pub fn load(
        path: impl AsRef<Path>,
        bank: u16,
        preset: u16,
        use_effects: bool,
    ) -> Result<Self, SoundFontError> {
        let presets = load_soundfont(path.as_ref(), 44_100)
            .map_err(|e| SoundFontError::Parse(format!("{e}")))?;

        let target = presets
            .iter()
            .find(|p| p.bank == bank && p.preset == preset)
            .ok_or(SoundFontError::MissingPreset(bank, preset))?;

        let mut sf = Self {
            bank,
            preset,
            samples: Vec::new(),
            zone_matrix: (0..128 * 128).map(|_| Vec::new()).collect(),
            zones: Vec::new(),
            sample_ids: HashMap::new(),
            use_effects,
            resample_cache: HashMap::new(),
        };

        for region in &target.regions {
            sf.add_region(region);
        }

        Ok(sf)
    }

    /// The loaded bank number.
    pub fn bank(&self) -> u16 {
        self.bank
    }

    /// The loaded preset number.
    pub fn preset(&self) -> u16 {
        self.preset
    }

    /// Returns the zone ids that apply to `(key, velocity)`, in SF2 priority
    /// order (the order they appear in the soundfont).
    pub fn zones_at(&self, key: u8, vel: u8) -> &[u16] {
        let idx = key as usize * 128 + vel as usize;
        &self.zone_matrix[idx]
    }

    /// Resolves a zone id returned by [`SoundFont::zones_at`].
    pub fn zone(&self, id: u16) -> &Zone {
        &self.zones[id as usize]
    }

    /// Number of unique sample arrays held by this soundfont.
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// The native-rate sample data for `sample_id`.
    pub fn sample_data(&self, id: usize) -> &Arc<[f32]> {
        &self.samples[id]
    }

    /// Lazily resamples sample `id` into `new_rate` using the same rubato
    /// sinc resampler as XSynth, and returns the resampled data.
    ///
    /// The result is cached per `(id, new_rate)` for the lifetime of the
    /// soundfont, so repeated calls are cheap.
    pub fn resample(&mut self, id: usize, new_rate: u32) -> Arc<[f32]> {
        let key = (id, new_rate);
        if let Some(data) = self.resample_cache.get(&key) {
            return data.clone();
        }
        let out = Self::resample_data(&self.samples[id], self.native_rate_for(id), new_rate);
        self.resample_cache.insert(key, out.clone());
        out
    }

    /// Computes the resampled data for sample `id` without touching the
    /// cache (safe to call concurrently from many threads). Use
    /// [`SoundFont::cache_resampled`] to store the result afterwards.
    pub fn resample_uncached(&self, id: usize, new_rate: u32) -> Arc<[f32]> {
        Self::resample_data(&self.samples[id], self.native_rate_for(id), new_rate)
    }

    /// Returns the cached resample for `id`, or computes it without caching
    /// when missing (safe to call concurrently; `cache_resampled` can store
    /// the result afterwards).
    pub fn resample_read(&self, id: usize, new_rate: u32) -> Arc<[f32]> {
        if let Some(data) = self.resample_cache.get(&(id, new_rate)) {
            return data.clone();
        }
        Self::resample_data(&self.samples[id], self.native_rate_for(id), new_rate)
    }

    fn resample_data(raw: &Arc<[f32]>, native: u32, new_rate: u32) -> Arc<[f32]> {
        xsynth_soundfonts::resample::resample_vec(raw.to_vec(), native as f32, new_rate as f32)
    }

    /// Stores a previously computed resample in the cache.
    pub fn cache_resampled(&mut self, id: usize, new_rate: u32, data: Arc<[f32]>) {
        self.resample_cache.insert((id, new_rate), data);
    }

    /// Pre-resamples all samples that a set of zone ids may use. This lets
    /// the engine upload everything in one batch before rendering.
    ///
    /// Returns a list of `(sample_id, resampled_len)` for the requested
    /// zones (deduplicated).
    pub fn ensure_resampled(&mut self, zone_ids: &[u16], new_rate: u32) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        let mut seen: Vec<bool> = Vec::new();
        for &zid in zone_ids {
            let zone = &self.zones[zid as usize];
            let id = zone.sample_id;
            if id >= seen.len() {
                seen.resize(id + 1, false);
            }
            if seen[id] {
                continue;
            }
            seen[id] = true;
            let data = self.resample(id, new_rate);
            out.push((id, data.len()));
        }
        out
    }

    fn native_rate_for(&self, id: usize) -> u32 {
        self.zones
            .iter()
            .find(|z| z.sample_id == id)
            .map(|z| z.native_rate)
            .unwrap_or(44_100)
    }

    fn add_region(&mut self, region: &xsynth_soundfonts::sf2::Sf2Region) {
        // Deduplicate sample data per channel (stereo pairs get two ids).
        let sample_channels = region.sample.len() as u32;
        let sample_id = self.dedup_samples_channel(region.sample.first().cloned());
        let sample_id_r = if sample_channels == 2 {
            self.dedup_samples_channel(region.sample.get(1).cloned())
        } else {
            sample_id
        };

        let native_rate = region.sample_rate;
        for key in region.keyrange.clone() {
            for vel in region.velrange.clone() {
                // note_params applies the baked note-on modulators (velocity
                // -> attenuation, velocity -> filter cutoff, key -> env).
                let params = region.note_params(key, vel);

                let tuned_key_cents =
                    (key as f32 - region.root_key as f32) * region.scale_tuning as f32;
                let speed_mult = cents_factor(
                    tuned_key_cents
                        + region.fine_tune as f32
                        + region.coarse_tune as f32 * 100.0
                        + params.tune_cents,
                );

                let cutoff = if self.use_effects {
                    params.cutoff.filter(|c| *c >= 1.0)
                } else {
                    None
                };

                let pan = ((params.pan as f32 / 500.0) + 1.0) / 2.0;

                let zone = Zone {
                    sample_id,
                    sample_id_r,
                    channels: sample_channels,
                    volume: params.volume,
                    pan,
                    speed_mult,
                    cutoff,
                    resonance_db: params.resonance,
                    loop_mode: if region.loop_start == region.loop_end {
                        LoopMode::NoLoop
                    } else {
                        region.loop_mode
                    },
                    loop_start: region.loop_start,
                    loop_end: region.loop_end,
                    offset: region.offset,
                    sample_end: region.sample_end,
                    envelope: envelope_from_ampeg(&params.ampeg_envelope),
                    exclusive_class: region.exclusive_class,
                    native_rate,
                };

                let zone_id = self.zones.len() as u16;
                self.zones.push(zone);
                let idx = key as usize * 128 + vel as usize;
                self.zone_matrix[idx].push(zone_id);
            }
        }
    }

    fn dedup_samples_channel(&mut self, sample: Option<Arc<[f32]>>) -> usize {
        if let Some(sample) = sample {
            let ptr = Arc::as_ptr(&sample) as *const () as usize as u64;
            if let Some(&id) = self.sample_ids.get(&ptr) {
                return id;
            }
            let id = self.samples.len();
            self.samples.push(sample);
            self.sample_ids.insert(ptr, id);
            return id;
        }
        usize::MAX
    }
}

fn envelope_from_ampeg(p: &xsynth_soundfonts::sfz::AmpegEnvelopeParams) -> EnvelopeDescriptor {
    EnvelopeDescriptor {
        start_percent: p.ampeg_start / 100.0,
        delay: p.ampeg_delay,
        attack: p.ampeg_attack,
        hold: p.ampeg_hold,
        decay: p.ampeg_decay,
        sustain_percent: p.ampeg_sustain / 100.0,
        release: p.ampeg_release,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_zone_params() {
        let sf = SoundFont::load("assets/test.sf2", 0, 0, true).unwrap();
        println!("samples: {}", sf.sample_count());
        let ids = sf.zones_at(60, 100);
        println!("zones at (60,100): {:?}", ids);
        for &id in ids.iter().take(4) {
            let z = sf.zone(id);
            println!(
                "zone {}: sample_id={} channels={} volume={:.6} pan={:.4} speed={:.6} cutoff={:?} res_db={} loop={:?} off={} end={} env(start={},delay={:.6},attack={:.6},hold={:.6},decay={:.6},sustain={:.4},release={:.6})",
                id,
                z.sample_id,
                z.channels,
                z.volume,
                z.pan,
                z.speed_mult,
                z.cutoff,
                z.resonance_db,
                z.loop_mode,
                z.offset,
                z.sample_end,
                z.envelope.start_percent,
                z.envelope.delay,
                z.envelope.attack,
                z.envelope.hold,
                z.envelope.decay,
                z.envelope.sustain_percent,
                z.envelope.release
            );
        }
    }
}
