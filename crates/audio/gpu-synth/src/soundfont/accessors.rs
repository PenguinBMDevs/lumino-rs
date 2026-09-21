use super::*;

impl SoundFont {
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
        if native == new_rate {
            // 数据已在目标率（SF2 修复后常见：加载即引擎率），免去 rubato 往返。
            return raw.clone();
        }
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
            .find(|z| z.sample_id == id || z.sample_id_r == id)
            .map(|z| z.native_rate)
            .unwrap_or(self.sample_rate)
    }
}
