use super::*;

/// 重采样缓存的字节上限（REF-DATA-001）。
///
/// 只计**非别名**条目：SF2 同率时缓存与 `core.samples` 共享 `Arc`，不计费。
/// 上限只约束 SFZ / 异率这类真正分配第二份数据的场景；256 MiB 约等于
/// 一个中型音源，超出后按 LRU 淘汰。
#[cfg(not(test))]
pub(crate) const RESAMPLE_CACHE_MAX_BYTES: u64 = 256 << 20;

/// 测试用小上限：用几 MB 的假数据即可触发淘汰（不改生产行为）。
#[cfg(test)]
pub(crate) const RESAMPLE_CACHE_MAX_BYTES: u64 = 1 << 20;

fn bytes_of(data: &Arc<[f32]>) -> u64 {
    (data.len() as u64).saturating_mul(4)
}

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
        let (native, resample) = self.memory_bytes_split();
        native.saturating_add(resample)
    }

    /// CPU 采样字节拆分 `(原生, 重采样缓存)`（REF-DATA-001）。
    ///
    /// 重采样部分已排除与原生共享 `Arc` 的同率条目，因此两数相加等于
    /// [`SoundFont::memory_bytes`]。
    pub fn memory_bytes_split(&self) -> (u64, u64) {
        let mut seen: HashSet<usize> = HashSet::new();
        let mut native = 0u64;
        for data in &self.core.samples {
            let ptr = Arc::as_ptr(data) as *const () as usize;
            if seen.insert(ptr) {
                native = native.saturating_add(bytes_of(data));
            }
        }
        let mut resample = 0u64;
        for entry in self.cache().map.values() {
            let ptr = Arc::as_ptr(&entry.data) as *const () as usize;
            if seen.insert(ptr) {
                resample = resample.saturating_add(bytes_of(&entry.data));
            }
        }
        (native, resample)
    }

    /// Lazily resamples sample `id` into `new_rate` using the same rubato
    /// sinc resampler as XSynth, and returns the resampled data.
    ///
    /// The result is cached per `(id, new_rate)` for the lifetime of the
    /// soundfont core, so repeated calls are cheap. The cache is bounded and
    /// LRU-evicted (REF-DATA-001); same-rate entries alias the native samples
    /// and never count toward the bound.
    pub fn resample(&mut self, id: usize, new_rate: u32) -> Arc<[f32]> {
        let key = (id, new_rate);
        if let Some(data) = self.cache_get(key) {
            return data;
        }
        let out = Self::resample_data(&self.core.samples[id], self.native_rate_for(id), new_rate);
        self.cache_insert(key, out.clone());
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
        if let Some(data) = self.cache_get((id, new_rate)) {
            return data;
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

    /// Stores a previously computed resample in the cache (bounded, LRU).
    pub fn cache_resampled(&mut self, id: usize, new_rate: u32, data: Arc<[f32]>) {
        self.cache_insert((id, new_rate), data);
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
    pub(crate) fn cache(&self) -> std::sync::MutexGuard<'_, ResampleCache> {
        self.core
            .resample_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// LRU 命中：更新 `last_used`，返回数据副本（锁只覆盖查找/更新）。
    fn cache_get(&self, key: (usize, u32)) -> Option<Arc<[f32]>> {
        let mut cache = self.cache();
        let tick = cache.tick + 1;
        let hit = cache.map.get_mut(&key).map(|entry| {
            entry.last_used = tick;
            entry.data.clone()
        });
        if hit.is_some() {
            cache.tick = tick;
        }
        hit
    }

    /// 插入缓存并执行 LRU 淘汰。
    ///
    /// 同率别名（与 `core.samples[id]` 同一 `Arc`）不计字节、也不作为淘汰
    /// 候选——淘汰它不会释放内存，只会让下次读取重新 clone。
    fn cache_insert(&self, key: (usize, u32), data: Arc<[f32]>) {
        let alias = self.is_alias(key.0, &data);
        let mut cache = self.cache();
        let tick = cache.tick + 1;
        cache.tick = tick;
        if let Some(old) = cache.map.insert(
            key,
            ResampleEntry {
                data: data.clone(),
                last_used: tick,
            },
        ) && !self.is_alias(key.0, &old.data)
        {
            cache.bytes = cache.bytes.saturating_sub(bytes_of(&old.data));
        }
        if !alias {
            cache.bytes = cache.bytes.saturating_add(bytes_of(&data));
        }
        while cache.bytes > RESAMPLE_CACHE_MAX_BYTES {
            let victim = cache
                .map
                .iter()
                .filter(|(k, e)| {
                    !self
                        .core
                        .samples
                        .get(k.0)
                        .is_some_and(|s| Arc::ptr_eq(s, &e.data))
                })
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| *k);
            let Some(victim) = victim else { break };
            if let Some(entry) = cache.map.remove(&victim) {
                cache.bytes = cache.bytes.saturating_sub(bytes_of(&entry.data));
            }
        }
    }

    fn is_alias(&self, id: usize, data: &Arc<[f32]>) -> bool {
        self.core
            .samples
            .get(id)
            .is_some_and(|s| Arc::ptr_eq(s, data))
    }
}
