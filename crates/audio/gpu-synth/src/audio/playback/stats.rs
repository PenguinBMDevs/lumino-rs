use super::*;

impl PlaybackStatsReader {
    /// The number of samples currently buffered (rendered but not yet
    /// consumed). Can be negative if the reader is waiting for samples.
    pub fn samples(&self) -> i64 {
        self.samples.load(Ordering::Relaxed)
    }

    /// The number of samples requested by the last audio callback.
    pub fn last_request_samples(&self) -> i64 {
        self.last_request_samples.load(Ordering::Relaxed)
    }

    /// The number of samples that were in the buffer after the last read.
    pub fn last_samples_after_read(&self) -> i64 {
        self.last_samples_after_read.load(Ordering::Relaxed)
    }

    /// The number of samples rendered per iteration.
    pub fn render_size(&self) -> usize {
        self.render_size.load(Ordering::Relaxed) as usize
    }

    /// The average render-time percentage (0 to 1) of how long the render
    /// thread spent rendering, relative to the max allowed time. Values
    /// above 1.0 mean the render thread cannot keep up with realtime.
    ///
    /// 等价于 [`Self::average_renderer_load_window`] 的全环口径
    /// （全环口径，现为 [`STATS_RING`] 块）。
    pub fn average_renderer_load(&self) -> f64 {
        self.average_renderer_load_window(STATS_RING)
    }

    /// 最近 `window` 块的渲染负载平均（短窗口径）。
    ///
    /// `window` 由调用方按「秒 × 速率 / 块大小」换算（如 3 秒），自动夹紧到
    /// `1..=STATS_RING`；不足 `window` 块时按已填充部分平均。未填充的槽位
    /// （bit pattern 为 0）跳过，与旧 `average_renderer_load` 的口径一致。
    pub fn average_renderer_load_window(&self, window: usize) -> f64 {
        let window = window.clamp(1, STATS_RING);
        let head = self.render_time_head.load(Ordering::Relaxed) as usize;
        let mut sum = 0.0f64;
        let mut n = 0usize;
        for i in 0..window {
            let bits =
                self.render_time[(head + STATS_RING - 1 - i) % STATS_RING].load(Ordering::Relaxed);
            if bits != 0 {
                sum += f64::from_bits(bits);
                n += 1;
            }
        }
        if n == 0 { 0.0 } else { sum / n as f64 }
    }

    /// The last render-time percentage (0 to 1).
    pub fn last_renderer_load(&self) -> f64 {
        let head = self.render_time_head.load(Ordering::Relaxed) as usize;
        let slot = (head + STATS_RING - 1) % STATS_RING;
        let bits = self.render_time[slot].load(Ordering::Relaxed);
        if bits == 0 { 0.0 } else { f64::from_bits(bits) }
    }

    /// Publishes one render-load sample (render thread side; lock-free).
    pub(crate) fn push_render_load(&self, load: f64) {
        let head = self.render_time_head.load(Ordering::Relaxed) as usize;
        self.render_time[head].store(load.to_bits(), Ordering::Relaxed);
        let next = (head + 1) % STATS_RING;
        self.render_time_head.store(next as u64, Ordering::Relaxed);
    }

    /// The active voice count reported by the engine on the last render.
    pub fn voice_count(&self) -> u64 {
        self.voice_count.load(Ordering::Relaxed)
    }

    /// The number of underruns (audio callbacks that found no buffered
    /// samples and wrote silence). Zero is the healthy state.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// REND-016 #139：被丢弃的过期 NoteOn 累计数。正常 / 中度密集素材应恒为 0；
    /// 仅在积压已深（事件时间追不回）时增长。
    pub fn dropped_note_ons(&self) -> u64 {
        self.dropped_note_ons.load(Ordering::Relaxed)
    }

    /// 累计被丢弃的过期 NoteOn（渲染线程侧，lock-free）。
    pub(crate) fn record_dropped_note_ons(&self, dropped: u64) {
        if dropped > 0 {
            self.dropped_note_ons.fetch_add(dropped, Ordering::Relaxed);
        }
    }

    /// REND-016 #139：当前治理级别（0=Normal / 1=High / 2=Overload / 3=Emergency）。
    pub fn governor_level(&self) -> u64 {
        self.governor_level.load(Ordering::Relaxed)
    }

    /// 发布当前治理级别（渲染线程侧，lock-free）。
    pub(crate) fn set_governor_level(&self, level: u64) {
        self.governor_level.store(level, Ordering::Relaxed);
    }

    /// 打包一次 GPU 缓冲状态快照（UI 低频展示用）。
    ///
    /// `window_samples` 为短窗平均的样本数（调用方按「秒 × 速率 / 块大小」
    /// 换算），由 [`Self::average_renderer_load_window`] 夹紧。
    /// 全部读取为 relaxed 原子，不触碰音频线程；队列负值（渲染落后）
    /// 在毫秒换算时钳到 0，原始值保留在 `queue_samples`。
    pub fn snapshot(&self, window_samples: usize) -> GpuBufferStats {
        let samples_per_second = self.samples_per_second.max(1);
        let queue_samples = self.samples();
        GpuBufferStats {
            queue_samples,
            queue_ms: queue_samples.max(0) as f64 / samples_per_second as f64 * 1000.0,
            render_load: self.last_renderer_load(),
            average_render_load: self.average_renderer_load_window(window_samples),
            voice_count: self.voice_count(),
            underruns: self.underruns(),
            governor_level: self.governor_level(),
            dropped_note_ons: self.dropped_note_ons(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造仅用于统计读取的 reader（不启动 GPU/音频设备）。
    fn test_reader(samples_per_second: u64) -> PlaybackStatsReader {
        PlaybackStatsReader {
            samples: Arc::new(AtomicI64::new(0)),
            last_request_samples: Arc::new(AtomicI64::new(0)),
            last_samples_after_read: Arc::new(AtomicI64::new(0)),
            render_time: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            render_time_head: Arc::new(AtomicU64::new(0)),
            render_size: Arc::new(AtomicU64::new(0)),
            samples_per_second,
            voice_count: Arc::new(AtomicU64::new(0)),
            underruns: Arc::new(AtomicU64::new(0)),
            dropped_note_ons: Arc::new(AtomicU64::new(0)),
            governor_level: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 短窗平均只取最近 k 个样本，窗口外的旧样本不得参与。
    #[test]
    fn test_average_renderer_load_window_uses_last_k_samples() {
        let reader = test_reader(96_000);
        // 旧样本：3 个高负载块（会被 2 样本窗口排除）
        for _ in 0..3 {
            reader.push_render_load(2.0);
        }
        // 最近 2 个块：0.5 / 0.7
        reader.push_render_load(0.5);
        reader.push_render_load(0.7);
        let avg = reader.average_renderer_load_window(2);
        assert!(
            (avg - 0.6).abs() < 1e-9,
            "2 样本窗口应只平均最近两块，得到 {avg}"
        );
        // 全环窗口应覆盖到旧样本（5 个样本均在环内）
        let all = reader.average_renderer_load_window(STATS_RING);
        let expected = (2.0 * 3.0 + 0.5 + 0.7) / 5.0;
        assert!(
            (all - expected).abs() < 1e-9,
            "全环平均应覆盖全部已填充样本，得到 {all}"
        );
        // 窗口为 0 时夹紧到 1（取最近一块）
        assert!((reader.average_renderer_load_window(0) - 0.7).abs() < 1e-9);
    }

    /// 队列毫秒换算按「交错样本/秒」口径；负值钳到 0。
    #[test]
    fn test_snapshot_converts_queue_samples_to_ms() {
        let reader = test_reader(96_000); // 48kHz × 2ch
        reader.samples.fetch_add(48_000, Ordering::Relaxed); // 0.5 秒
        let stats = reader.snapshot(375);
        assert_eq!(stats.queue_samples, 48_000);
        assert!(
            (stats.queue_ms - 500.0).abs() < 1e-9,
            "48000 交错样本 @96k/s 应为 500ms，得到 {}",
            stats.queue_ms
        );

        // 渲染落后（负值）：毫秒钳到 0，原始值保留
        reader.samples.store(-9_600, Ordering::Relaxed);
        let stats = reader.snapshot(375);
        assert_eq!(stats.queue_samples, -9_600);
        assert_eq!(stats.queue_ms, 0.0);
    }
}
