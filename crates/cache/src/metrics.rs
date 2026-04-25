//! 性能指标收集
//!
//! 用于监控缓存行为的核心指标：
//! - 各层命中率
//! - 同步加载延迟
//! - 预取成功率
//! - 内存占用
//!
//! 这些指标用于后续调优（见 tuning.md）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 缓存性能指标
#[derive(Debug, Default)]
pub struct CacheMetrics {
    // L1 命中/未命中
    pub l1_hits: AtomicU64,
    pub l1_misses: AtomicU64,

    // L2 命中/未命中
    pub l2_hits: AtomicU64,
    pub l2_misses: AtomicU64,

    // L3 读取
    pub l3_reads: AtomicU64,

    // 同步加载（兜底路径）
    pub sync_loads: AtomicU64,
    pub sync_load_ns: AtomicU64,

    // 预取
    pub prefetch_hits: AtomicU64,
    pub prefetch_misses: AtomicU64,
    pub prefetch_loads: AtomicU64,
}

impl CacheMetrics {
    /// 创建新的指标收集器
    pub const fn new() -> Self {
        Self {
            l1_hits: AtomicU64::new(0),
            l1_misses: AtomicU64::new(0),
            l2_hits: AtomicU64::new(0),
            l2_misses: AtomicU64::new(0),
            l3_reads: AtomicU64::new(0),
            sync_loads: AtomicU64::new(0),
            sync_load_ns: AtomicU64::new(0),
            prefetch_hits: AtomicU64::new(0),
            prefetch_misses: AtomicU64::new(0),
            prefetch_loads: AtomicU64::new(0),
        }
    }

    /// L1 命中率
    pub fn l1_hit_rate(&self) -> f64 {
        let hits = self.l1_hits.load(Ordering::Relaxed);
        let total = hits + self.l1_misses.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// L2 命中率
    pub fn l2_hit_rate(&self) -> f64 {
        let hits = self.l2_hits.load(Ordering::Relaxed);
        let total = hits + self.l2_misses.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// 预取命中率
    pub fn prefetch_hit_rate(&self) -> f64 {
        let hits = self.prefetch_hits.load(Ordering::Relaxed);
        let total = hits + self.prefetch_misses.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// 平均同步加载延迟（微秒）
    pub fn avg_sync_load_us(&self) -> f64 {
        let loads = self.sync_loads.load(Ordering::Relaxed);
        if loads == 0 {
            return 0.0;
        }
        let total_ns = self.sync_load_ns.load(Ordering::Relaxed);
        (total_ns as f64 / loads as f64) / 1000.0
    }

    /// 记录 L1 命中
    #[inline]
    pub fn record_l1_hit(&self) {
        self.l1_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录 L1 未命中
    #[inline]
    pub fn record_l1_miss(&self) {
        self.l1_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录 L2 命中
    #[inline]
    pub fn record_l2_hit(&self) {
        self.l2_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录 L2 未命中
    #[inline]
    pub fn record_l2_miss(&self) {
        self.l2_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录 L3 读取
    #[inline]
    pub fn record_l3_read(&self) {
        self.l3_reads.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录同步加载（包含延迟）
    pub fn record_sync_load(&self, duration: Duration) {
        self.sync_loads.fetch_add(1, Ordering::Relaxed);
        self.sync_load_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    /// 记录预取成功
    #[inline]
    pub fn record_prefetch_hit(&self) {
        self.prefetch_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录预取未命中
    #[inline]
    pub fn record_prefetch_miss(&self) {
        self.prefetch_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录预取加载
    #[inline]
    pub fn record_prefetch_load(&self) {
        self.prefetch_loads.fetch_add(1, Ordering::Relaxed);
    }

    /// 生成指标报告文本
    pub fn report(&self) -> String {
        format!(
            concat!(
                "═══ 缓存性能指标 ═══\n",
                "L1 命中率:  {:.1}% ({} hits / {})\n",
                "L2 命中率:  {:.1}% ({} hits / {})\n",
                "L3 读取数:  {}\n",
                "预取命中率: {:.1}% ({} hits / {})\n",
                "同步加载:    {} 次, 平均 {:.1} μs\n",
            ),
            self.l1_hit_rate() * 100.0,
            self.l1_hits.load(Ordering::Relaxed),
            self.l1_hits.load(Ordering::Relaxed) + self.l1_misses.load(Ordering::Relaxed),
            self.l2_hit_rate() * 100.0,
            self.l2_hits.load(Ordering::Relaxed),
            self.l2_hits.load(Ordering::Relaxed) + self.l2_misses.load(Ordering::Relaxed),
            self.l3_reads.load(Ordering::Relaxed),
            self.prefetch_hit_rate() * 100.0,
            self.prefetch_hits.load(Ordering::Relaxed),
            self.prefetch_hits.load(Ordering::Relaxed)
                + self.prefetch_misses.load(Ordering::Relaxed),
            self.sync_loads.load(Ordering::Relaxed),
            self.avg_sync_load_us(),
        )
    }

    /// 重置所有指标
    pub fn reset(&self) {
        self.l1_hits.store(0, Ordering::Relaxed);
        self.l1_misses.store(0, Ordering::Relaxed);
        self.l2_hits.store(0, Ordering::Relaxed);
        self.l2_misses.store(0, Ordering::Relaxed);
        self.l3_reads.store(0, Ordering::Relaxed);
        self.sync_loads.store(0, Ordering::Relaxed);
        self.sync_load_ns.store(0, Ordering::Relaxed);
        self.prefetch_hits.store(0, Ordering::Relaxed);
        self.prefetch_misses.store(0, Ordering::Relaxed);
        self.prefetch_loads.store(0, Ordering::Relaxed);
    }
}

/// 用于测量操作持续时间的计时器
pub struct ScopeTimer {
    start: Instant,
    metrics: &'static CacheMetrics,
}

impl ScopeTimer {
    /// 创建新的计时器
    pub fn new(metrics: &'static CacheMetrics) -> Self {
        Self {
            start: Instant::now(),
            metrics,
        }
    }

    /// 停止计时并记录同步加载延迟
    pub fn record_sync_load(self) {
        let elapsed = self.start.elapsed();
        self.metrics.record_sync_load(elapsed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_start_empty() {
        let m = CacheMetrics::new();
        assert_eq!(m.l1_hit_rate(), 0.0);
        assert_eq!(m.avg_sync_load_us(), 0.0);
    }

    #[test]
    fn test_metrics_hit_rate() {
        let m = CacheMetrics::new();
        m.record_l1_hit();
        m.record_l1_hit();
        m.record_l1_miss();
        assert!((m.l1_hit_rate() - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_metrics_report_format() {
        let m = CacheMetrics::new();
        let report = m.report();
        assert!(report.contains("L1"));
        assert!(report.contains("L2"));
        assert!(report.contains("L3"));
    }

    #[test]
    fn test_metrics_reset() {
        let m = CacheMetrics::new();
        m.record_l1_hit();
        assert_eq!(m.l1_hits.load(Ordering::Relaxed), 1);
        m.reset();
        assert_eq!(m.l1_hits.load(Ordering::Relaxed), 0);
    }
}
