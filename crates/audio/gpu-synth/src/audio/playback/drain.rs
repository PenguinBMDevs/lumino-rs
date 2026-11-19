//! REND-016 #139：渲染线程的事件积压治理与负载治理器。
//!
//! GPU 特有的杀手是「事件积压一次性注入」：大块渲染结束后把积压的上万事件
//! 全部 drain 进下一块 → 复音更高 → 块更大 → 级联，最终缓冲队列耗尽（欠载
//! 无声）。本模块给 GPU 补上「双闸 + 反馈」：
//!
//! - **事件闸（有界 drain）**：每块最多 [`MAX_EVENTS_PER_BLOCK`] 条且受时间
//!   预算约束（块周期 × [`DRAIN_DUTY`]，到点即停，块耗时恒定）；最老事件
//!   年龄超过 [`EMERGENCY_DRAIN_MULTIPLIER`]×deadline 时放宽到
//!   [`FLUSH_MAX_EVENTS`] 与 2× 时间预算，尽快冲掉积压；
//! - **过期丢弃**：`age > deadline` 的 NoteOn 丢弃（时间已追不回，保其余事件
//!   的时间对齐）；NoteOff / 状态类事件永不丢（防挂音 / 上下文错乱）；
//! - **反馈闸（Governor）**：按 render-load EMA 与「时间已落后」证据分四级
//!   （Normal/High/Overload/Emergency）；Overload/Emergency 对本块**新鲜
//!   NoteOn** 施加准入预算（[`OVERLOAD_NOTE_ON_BUDGET`] /
//!   [`EMERGENCY_NOTE_ON_BUDGET`]），防止声部风暴持续注入；含迟滞与冷却，
//!   避免「丢弃→负载降→退出→再进入」的密度泵动。

use super::*;

mod governor;

pub(crate) use governor::{Governor, GovernorLevel, voice_limits_for};

/// 单块事件 drain 的计数上限（正常路径）。
const MAX_EVENTS_PER_BLOCK: usize = 4_096;
/// 正常路径单块 drain 的时间预算**下限**（块小时保持此值）。
const DRAIN_TIME_BUDGET_MIN: Duration = Duration::from_millis(4);
/// drain 时间预算 = 块周期 × 此占比（**块大时随周期放大**，保事件吞吐容量
/// 恒定——固定 4ms 在 8192 块（周期 128ms）下 duty 仅 3.5%，容量塌方 →
/// 积压无限涨 → 现场"8192 没法听"）。
const DRAIN_DUTY: f64 = 0.30;
/// 正常路径单块 drain 的时间预算上限。
const DRAIN_TIME_BUDGET_MAX: Duration = Duration::from_millis(32);
/// 紧急冲洗时间预算 = 正常预算 × 此倍数。
const DRAIN_FLUSH_MULTIPLIER: u32 = 2;
/// 时间检查间隔（事件数）：摊销 `Instant::now()` 成本。
const DRAIN_TIME_CHECK_EVERY: usize = 256;
/// 紧急路径：最老事件年龄超过该倍数 deadline 时放宽单块上限。
///
/// 取 2×（1024@48k ≈ 100ms）：尽早进入冲洗/治理，减少"闸还没开、队列已耗尽"
/// 的前置静音窗口。
const EMERGENCY_DRAIN_MULTIPLIER: u32 = 2;
/// 紧急冲洗（L4 积压全清）单次扫描上限。
///
/// 积压已深时（最老事件年龄超 4×deadline）一次性扫完整个过期区：过期
/// NoteOn 丢弃、NoteOff/状态事件照常投递，渲染线程直接跳回新鲜区——把
/// 「积压追平期间数秒级的静音」压缩到一次扫描（百万级 ≈ 0.1-0.2s）。
const FLUSH_MAX_EVENTS: usize = 2_000_000;
/// deadline = 3×块时长（1024@48k ≈ 64ms）。
const DEADLINE_BLOCKS: u64 = 3;
/// deadline 下限：块极小时防止误杀正常事件。
const DEADLINE_FLOOR_MS: u64 = 50;

/// 事件年龄 deadline：3×块时长，且不小于 [`DEADLINE_FLOOR_MS`]。
pub(crate) fn event_deadline(block: usize, sample_rate: u32) -> Duration {
    let block_ns = (block as u64).saturating_mul(1_000_000_000) / u64::from(sample_rate.max(1));
    Duration::from_nanos(block_ns.saturating_mul(DEADLINE_BLOCKS))
        .max(Duration::from_millis(DEADLINE_FLOOR_MS))
}

/// 是否可过期丢弃：只有 NoteOn 可丢。
///
/// NoteOff 丢了会挂音；状态类事件（CC/PC/PB 等）丢了会上下文错乱——两者永不丢。
fn is_droppable(event: &MidiEvent) -> bool {
    matches!(event, MidiEvent::NoteOn { .. })
}

/// 单块 drain 的结果（供治理器反馈与统计）。
pub(crate) struct DrainOutcome {
    /// 本块实际处理（含丢弃）的事件数。
    pub(crate) processed: usize,
    /// 过期丢弃的 NoteOn 数。
    pub(crate) dropped_expired: u64,
    /// 准入预算丢弃的新鲜 NoteOn 数。
    pub(crate) dropped_budget: u64,
    /// 是否触发了紧急放宽（积压已深，供治理器直入 Emergency）。
    pub(crate) emergency_evidence: bool,
}

/// 单块 drain 的时间预算：块周期 × [`DRAIN_DUTY`]，夹在
/// [`DRAIN_TIME_BUDGET_MIN`] 与 [`DRAIN_TIME_BUDGET_MAX`] 之间（块大时随周期
/// 放大，保事件吞吐容量恒定）。
pub(crate) fn drain_time_budget(block_period: Duration) -> Duration {
    block_period
        .mul_f64(DRAIN_DUTY)
        .clamp(DRAIN_TIME_BUDGET_MIN, DRAIN_TIME_BUDGET_MAX)
}

/// 有界 drain 一帧积压事件（语义见模块文档）。
///
/// 准入预算来自 `governor`；`stats` 记录累计丢弃；`block_period` 用于推导
/// 时间预算（块周期 × [`DRAIN_DUTY`]，随块大小自适应）。
pub(crate) fn drain_events(
    rx: &mpsc::Receiver<StampedEvent>,
    now: Instant,
    deadline: Duration,
    block_period: Duration,
    governor: &Governor,
    stats: &PlaybackStatsReader,
    on_event: impl FnMut(u8, MidiEvent),
) -> DrainOutcome {
    drain_events_with_budget(
        rx,
        now,
        deadline,
        drain_time_budget(block_period),
        governor,
        stats,
        on_event,
    )
}

/// [`drain_events`] 的实现：正常路径时间预算**显式传入**，测试可绕开墙钟。
///
/// 计数闸（[`MAX_EVENTS_PER_BLOCK`]）、紧急冲洗与准入预算的语义都不依赖真实
/// 时钟；把墙钟 4.8ms 预算留在测试里会让断言随 runner 负载抖动（CI 慢机上
/// `drain_start.elapsed() > 4.8ms` 在 256 条处就成立，计数闸根本到不了 4096）。
fn drain_events_with_budget(
    rx: &mpsc::Receiver<StampedEvent>,
    now: Instant,
    deadline: Duration,
    normal_budget: Duration,
    governor: &Governor,
    stats: &PlaybackStatsReader,
    mut on_event: impl FnMut(u8, MidiEvent),
) -> DrainOutcome {
    let mut cap = MAX_EVENTS_PER_BLOCK;
    let mut processed = 0usize;
    let mut dropped_expired = 0u64;
    let mut dropped_budget = 0u64;
    let mut admitted_note_ons = 0usize;
    let mut emergency_evidence = false;
    let mut flush_mode = false;
    let mut note_offs = 0u64;
    let mut controls = 0u64;
    let mut oldest_ms = 0u64;
    let budget = governor.note_on_budget();
    // REND-016 #139：时间预算（块耗时恒定，防释放墙拖慢块）。
    let drain_start = Instant::now();
    let flush_budget = normal_budget * DRAIN_FLUSH_MULTIPLIER;
    let mut time_budget = normal_budget;

    while processed < cap {
        // 时间预算：到点即停（剩余留待下块）。每 256 条检查一次，摊销时钟成本。
        if processed.is_multiple_of(DRAIN_TIME_CHECK_EVERY) && drain_start.elapsed() > time_budget {
            break;
        }
        let Ok((channel, event, enqueued_at)) = rx.try_recv() else {
            break;
        };
        processed += 1;
        let age = now.saturating_duration_since(enqueued_at);
        if processed == 1 {
            oldest_ms = age.as_millis() as u64;
        }
        match &event {
            MidiEvent::NoteOff { .. } => note_offs += 1,
            MidiEvent::NoteOn { .. } => {}
            _ => controls += 1,
        }

        // 紧急冲洗（REND-016 #139）：FIFO 下首个事件即最老事件，其年龄超限
        // 说明积压已深——本块按放宽的时间预算扫完过期区（而不是每块只扫
        // 65536 条），让渲染线程尽快回到新鲜区，消除"追平期间的长静音"。
        if processed == 1 && age > deadline * EMERGENCY_DRAIN_MULTIPLIER {
            cap = FLUSH_MAX_EVENTS;
            time_budget = flush_budget;
            emergency_evidence = true;
            flush_mode = true;
        }

        if is_droppable(&event) {
            if age > deadline {
                dropped_expired += 1;
                continue;
            }
            if budget.is_some_and(|b| admitted_note_ons >= b) {
                dropped_budget += 1;
                continue;
            }
            admitted_note_ons += 1;
        }
        on_event(channel, event);
    }

    if flush_mode {
        let elapsed_ms = drain_start.elapsed().as_millis();
        tracing::warn!(
            "[EVENT-FLUSH] 紧急冲洗：扫描 {} 条（NoteOff {} / 控制 {} / 过期 NoteOn {} / 预算 {}），最老 {}ms，耗时 {}ms（累计 {}，L{}）",
            processed,
            note_offs,
            controls,
            dropped_expired,
            dropped_budget,
            oldest_ms,
            elapsed_ms,
            stats.dropped_note_ons() + dropped_expired,
            governor.level() as u8
        );
        eprintln!(
            "[EVENT-FLUSH] 紧急冲洗：扫描 {} 条（NoteOff {} / 控制 {} / 过期 NoteOn {} / 预算 {}），最老 {}ms，耗时 {}ms（累计 {}，L{}）",
            processed,
            note_offs,
            controls,
            dropped_expired,
            dropped_budget,
            oldest_ms,
            elapsed_ms,
            stats.dropped_note_ons() + dropped_expired,
            governor.level() as u8
        );
    }
    let total_dropped = dropped_expired + dropped_budget;
    if total_dropped > 0 {
        stats.record_dropped_note_ons(total_dropped);
        log_drop_rate_limited(dropped_expired, dropped_budget, governor.level(), stats);
    }
    DrainOutcome {
        processed,
        dropped_expired,
        dropped_budget,
        emergency_evidence,
    }
}

/// 限频（500ms）输出丢弃告警；与 `[UNDERRUN]` 同款 stderr 标记，便于现场诊断。
fn log_drop_rate_limited(
    expired: u64,
    budget: u64,
    level: GovernorLevel,
    stats: &PlaybackStatsReader,
) {
    static LAST_DROP_LOG: AtomicI64 = AtomicI64::new(0);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let prev = LAST_DROP_LOG.load(Ordering::Relaxed);
    if ms - prev > 500 {
        LAST_DROP_LOG.store(ms, Ordering::Relaxed);
        // 双通道：tracing 进文件日志（GUI release 唯一可见），eprintln 供控制台构建。
        tracing::warn!(
            "[EVENT-DROP] 过期 {} + 预算 {} 条（累计 {}，L{}）",
            expired,
            budget,
            stats.dropped_note_ons(),
            level as u8
        );
        eprintln!(
            "[EVENT-DROP] 过期 {} + 预算 {} 条（累计 {}，L{}）",
            expired,
            budget,
            stats.dropped_note_ons(),
            level as u8
        );
    }
}

#[cfg(test)]
mod tests;
