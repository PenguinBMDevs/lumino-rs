use crate::realtime::ThreadCount as LuminoThreadCount;

use super::THREAD_POOL_ENV;

/// 探测进程可用逻辑核数（至少为 1）。
pub(super) fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// 解析线程池逃生口（纯函数，便于单测）。
///
/// - 未设置 / `0` / `none` / `off` / `false` / 非法值 → `ThreadCount::None`（默认策略）；
/// - `manual` / `on` / `true` → `Manual(逻辑核数)`（复现历史行为，用于在高核机器上对比）；
/// - 正整数 → `Manual(n)`（指定池线程数）。
pub(super) fn parse_thread_pool_override(
    value: Option<&str>,
    logical_cores: usize,
) -> LuminoThreadCount {
    match value.map(str::trim) {
        None | Some("") | Some("0") | Some("none") | Some("off") | Some("false") => {
            LuminoThreadCount::None
        }
        Some("manual") | Some("on") | Some("true") => {
            LuminoThreadCount::Manual(logical_cores.max(1))
        }
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|threads| *threads > 0)
            .map(LuminoThreadCount::Manual)
            .unwrap_or(LuminoThreadCount::None),
    }
}

/// 线程策略：**始终不使用通道内并行池**（`ThreadCount::None`）。
///
/// 每通道一个独立渲染线程（16 通道 = 16 线程），通道之间互不耦合。
///
/// 为什么不按核数切换到通道内 rayon 池（历史 `>16 逻辑核 → Manual(核数)` 分支）：
///
/// 1. 该池是**全部 16 个通道共享**的（`prepare_channels` 里 `Arc<ThreadPool>` 被克隆给每个
///    通道），因此并不提供"每通道更多并行"，只是把 16 个通道串行排进同一个池 —— 队头阻塞。
///    既有实测（16 通道并发 × 每通道 ~512 voices、100ms 块、12/6/4/2 逻辑核四档）：
///    共享池最坏单块 0.96~4.6s，无池最坏单块 0.14~0.57s，无池全面不劣。
/// 2. `ThreadCount::Manual(_)` 会让 xsynth 侧 `set_batch_render_available(false)`
///    （见 `RealtimeSynth::open`），**连带关闭 B1 跨 voice 批渲染**——那是 fork 的核心优化。
///    于是 >16 核机器同时失去批渲染、又背上队头阻塞，是双重劣势。
/// 3. 高核机器上剩余的核并非没有用处：缓冲渲染线程、音频回调、播放线程与 UI 都在抢核，
///    `>16` 分支把"额外核可用"直接等同于"通道内并行更划算"，缺少实测支撑。
///
/// 因此本函数不再按核数分支：**任何核数都走无池 + 批渲染**。
/// 需要在真实高核机器上复测尾延迟时，用 `XSYNTH_THREAD_POOL=manual` 一键切回池化做 A/B。
pub(super) fn forced_thread_count(logical_cores: usize) -> LuminoThreadCount {
    let raw = std::env::var(THREAD_POOL_ENV).ok();
    let mode = parse_thread_pool_override(raw.as_deref(), logical_cores);
    if !matches!(mode, LuminoThreadCount::None) {
        tracing::warn!(
            "XSynth: {THREAD_POOL_ENV}={:?} 覆盖线程策略 → {mode:?}（仅供 A/B 复测，会关闭批渲染）",
            raw.unwrap_or_default()
        );
    }
    mode
}

/// 按当前机器强制解析线程模式（供 `init_synth` 使用）。
pub(super) fn machine_thread_count() -> LuminoThreadCount {
    let logical = available_parallelism();
    tracing::debug!("XSynth: 固定线程策略（{logical} 逻辑核，无通道内并行池、启用批渲染）");
    forced_thread_count(logical)
}
