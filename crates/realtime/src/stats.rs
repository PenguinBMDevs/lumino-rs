//! 统计信息

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// 渲染性能统计
#[derive(Debug, Clone, Copy)]
pub struct RenderPerfStats {
    /// 最后一次渲染耗时（微秒）
    pub last_render_us: i64,
    /// 峰值渲染耗时（微秒）
    pub peak_render_us: i64,
    /// 最后一次渲染的事件数
    pub last_event_count: u64,
}

/// 共享的性能计数器
pub(crate) struct RenderPerfShared {
    pub(crate) last_render_ns: AtomicU64,
    pub(crate) peak_render_ns: AtomicU64,
    pub(crate) last_event_count: AtomicU64,
    /// 平均渲染负载 (0.0 - 1.0+)，以 f64 bits 存储
    pub(crate) average_load: AtomicU64,
}

impl RenderPerfShared {
    pub fn new() -> Self {
        Self {
            last_render_ns: AtomicU64::new(0),
            peak_render_ns: AtomicU64::new(0),
            last_event_count: AtomicU64::new(0),
            average_load: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> RenderPerfStats {
        let last = self.last_render_ns.load(Ordering::Relaxed);
        let peak = self.peak_render_ns.load(Ordering::Relaxed);
        RenderPerfStats {
            last_render_us: last as i64 / 1000,
            peak_render_us: peak as i64 / 1000,
            last_event_count: self.last_event_count.load(Ordering::Relaxed),
        }
    }
}

/// 实时合成器统计信息
#[derive(Debug, Clone)]
pub(crate) struct RealtimeSynthStats {
    pub(crate) voice_count: Arc<AtomicU64>,
}

impl RealtimeSynthStats {
    pub fn new() -> Self {
        Self {
            voice_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// 实时合成器统计信息读取器（不可变快照）
#[derive(Debug, Clone, Copy)]
pub struct RealtimeSynthStatsReader {
    /// 当前活跃 voice 数量
    pub voice_count: u64,
    /// 渲染器平均负载 (0.0 - 1.0+)
    pub average_renderer_load: f64,
    /// 缓冲区样本数（直接渲染模式下恒为 0）
    pub last_samples_after_read: i64,
}
