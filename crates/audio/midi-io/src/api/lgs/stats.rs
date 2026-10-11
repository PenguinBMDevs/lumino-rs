//! LGS (GPU) 缓冲状态快照换算。
//!
//! 自 `lgs.rs` 拆出（保持单文件长度纪律）；把「短窗平均」的秒数换算成
//! 样本数，并打包 `PlaybackStatsReader` 快照供 Runner/UI 低频展示。

use super::*;

/// 短窗平均负载的窗口时长（秒）。
///
/// 状态栏 LOAD 的「短窗」口径：3 秒。按块周期换算成样本数后交给
/// `PlaybackStatsReader::average_renderer_load_window`（其自动夹紧到环容量）。
pub(super) const LOAD_SHORT_WINDOW_SECS: f64 = 3.0;

/// 把窗口时长（秒）换算成渲染负载环的样本数：`秒 × 采样率 / 块大小`。
///
/// 默认配置 3s @ 512/64k → 375 块（约 3 秒墙钟）；小块配置样本数更大，
/// 由 reader 侧夹紧到 `STATS_RING = 1024`（如 64 块时封顶 ≈1 秒）。
pub(super) fn load_window_samples(block_size: usize, sample_rate: u32) -> usize {
    let block = block_size.max(1) as f64;
    ((LOAD_SHORT_WINDOW_SECS * f64::from(sample_rate)) / block)
        .round()
        .max(1.0) as usize
}

impl Lgs {
    /// 打包当前缓冲状态快照（无锁原子读取）。
    pub(super) fn buffer_stats(&self) -> GpuBufferStats {
        self.stats.snapshot(load_window_samples(
            self.options.block_size,
            self.options.sample_rate,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 窗口换算随块周期缩放：默认 512@64k → 375 块（≈3 秒）。
    #[test]
    fn test_load_window_samples_scales_with_block_period() {
        assert_eq!(load_window_samples(512, 64_000), 375, "默认配置 3 秒窗");
        assert_eq!(load_window_samples(1024, 64_000), 188, "块翻倍 → 样本减半");
        assert_eq!(load_window_samples(8192, 64_000), 23, "大块 → 少量样本");
        assert_eq!(
            load_window_samples(0, 64_000),
            192_000,
            "块大小为 0 时按 1 块防御，不得除零"
        );
    }
}
