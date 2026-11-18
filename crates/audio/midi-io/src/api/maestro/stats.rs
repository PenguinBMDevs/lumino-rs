//! Maestro 统计类型与读取器（从 `maestro.rs` 拆出，保持单文件 < 400 行）。

use maestro_api_rs::Stats;

use super::MaestroShared;

/// Maestro 实时统计快照（与 `maestro-api-rs::Stats` 解耦，避免上层依赖其类型）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaestroStats {
    /// 当前活跃声部数。
    pub voice_count: u64,
    /// 渲染负载（100 = 一个 render block 的完整预算）。
    pub render_time: f32,
    /// 每秒处理的音符数。
    pub nps: u64,
    /// 每秒处理的事件数。
    pub eps: u64,
    /// 当前 NPS 上限（Maestro 侧负载限流）。
    pub nps_limit: u64,
    /// 从事件时间戳到实际发声的延迟。
    pub latency_us: u64,
}

impl From<Stats> for MaestroStats {
    fn from(stats: Stats) -> Self {
        Self {
            voice_count: stats.voice_count,
            render_time: stats.render_time,
            nps: stats.nps,
            eps: stats.eps,
            nps_limit: stats.nps_limit,
            latency_us: u64::try_from(stats.latency.as_micros()).unwrap_or(u64::MAX),
        }
    }
}

/// 无锁/短锁读取 Maestro 统计；可在 UI 线程低频调用。
#[derive(Clone)]
pub struct MaestroStatsReader {
    pub(super) shared: std::sync::Arc<MaestroShared>,
}

impl MaestroStatsReader {
    /// 读取实时统计；引擎重建中/未运行时返回 `None`。
    pub fn stats(&self) -> Option<MaestroStats> {
        let rt = self.shared.current_engine()?;
        rt.stats().ok().map(MaestroStats::from)
    }

    /// 取走一条待展示的引擎通知（禁用/停止），供状态栏提示。
    pub fn take_notice(&self) -> Option<String> {
        self.shared.notice.lock().take()
    }
}
