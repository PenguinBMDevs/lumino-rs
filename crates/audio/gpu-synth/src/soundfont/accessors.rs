use super::*;

impl SoundFont {
    /// The loaded bank number.
    pub fn bank(&self) -> u16 {
        self.core.bank
    }

    /// The loaded preset number.
    pub fn preset(&self) -> u16 {
        self.core.preset
    }

    /// Returns the zone ids that apply to `(key, velocity)`, in SF2 priority
    /// order (the order they appear in the soundfont).
    pub fn zones_at(&self, key: u8, vel: u8) -> &[u16] {
        let idx = key as usize * 128 + vel as usize;
        &self.core.zone_matrix[idx]
    }

    /// Resolves a zone id returned by [`SoundFont::zones_at`].
    pub fn zone(&self, id: u16) -> &Zone {
        &self.core.zones[id as usize]
    }

    /// Number of unique sample arrays held by this soundfont.
    pub fn sample_count(&self) -> usize {
        self.core.samples.len()
    }

    /// The native-rate sample data for `sample_id`.
    pub fn sample_data(&self, id: usize) -> &Arc<[f32]> {
        &self.core.samples[id]
    }

    /// 采样数据占用的 CPU 字节：原生样本 + 重采样缓存（REF-LGS-013）。
    ///
    /// 只统计 f32 采样本体，不含 zone/矩阵/哈希表等元数据；用于状态栏的
    /// 显存/内存趋势展示，不作为精确的内存计量。
    ///
    /// REF-LGS-014：按 `Arc` 指针去重——同采样率时重采样缓存直接复用原生
    /// 采样（`resample_data` 的 fast path），不去重会把同一份数据算两遍。
    pub fn memory_bytes(&self) -> u64 {
        let mut seen: HashSet<usize> = HashSet::new();
        let mut total: u64 = 0;
        let add = |data: &Arc<[f32]>, total: &mut u64, seen: &mut HashSet<usize>| {
            let ptr = Arc::as_ptr(data) as *const () as usize;
            if seen.insert(ptr) {
                *total = total.saturating_add((data.len() as u64).saturating_mul(4));
            }
        };
        for data in &self.core.samples {
            add(data, &mut total, &mut seen);
        }
        for data in self.cache().values() {
            add(data, &mut total, &mut seen);
        }
        total
    }

    /// Lazily resamples sample `id` into `new_rate` using the same rubato
    /// sinc resampler as XSynth, and returns the resampled data.
    ///
    /// The result is cached per `(id, new_rate)` for the lifetime of the
    /// soundfont core, so repeated calls are cheap.
    pub fn resample(&mut self, id: usize, new_rate: u32) -> Arc<[f32]> {
        let key = (id, new_rate);
        if let Some(data) = self.cache().get(&key) {
            return data.clone();
        }
        let out = Self::resample_data(&self.core.samples[id], self.native_rate_for(id), new_rate);
        self.cache().insert(key, out.clone());
        out
    }

    /// Computes the resampled data for sample `id` without touching the
    /// cache (safe to call concurrently from many threads). Use
    /// [`SoundFont::cache_resampled`] to store the result afterwards.
    pub fn resample_uncached(&self, id: usize, new_rate: u32) -> Arc<[f32]> {
        Self::resample_data(&self.core.samples[id], self.native_rate_for(id), new_rate)
    }

    /// Returns the cached resample for `id`, or computes it without caching
    /// when missing (safe to call concurrently; `cache_resampled` can store
    /// the result afterwards).
    pub fn resample_read(&self, id: usize, new_rate: u32) -> Arc<[f32]> {
        if let Some(data) = self.cache().get(&(id, new_rate)) {
            return data.clone();
        }
        Self::resample_data(&self.core.samples[id], self.native_rate_for(id), new_rate)
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
        self.cache().insert((id, new_rate), data);
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
            let zone = &self.core.zones[zid as usize];
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
        self.core
            .zones
            .iter()
            .find(|z| z.sample_id == id || z.sample_id_r == id)
            .map(|z| z.native_rate)
            .unwrap_or(self.core.sample_rate)
    }

    /// Locks the (core-shared) resample cache, recovering from poisoning: the
    /// map holds only cached `Arc`s, so a poisoned lock cannot corrupt audio.
    fn cache(&self) -> std::sync::MutexGuard<'_, ResampleMap> {
        self.core
            .resample_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}
