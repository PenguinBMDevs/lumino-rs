//! REND-016 #139：渲染线程的事件积压治理与负载治理器。
//!
//! GPU 特有的杀手是「事件积压一次性注入」：大块渲染结束后把积压的上万事件
//! 全部 drain 进下一块 → 复音更高 → 块更大 → 级联，最终缓冲队列耗尽（欠载
//! 无声）。本模块给 GPU 补上「双闸 + 反馈」：
//!
//! - **事件闸（有界 drain）**：每块最多 [`MAX_EVENTS_PER_BLOCK`] 条；最老事件
//!   年龄超过 [`EMERGENCY_DRAIN_MULTIPLIER`]×deadline 时放宽到
//!   [`EMERGENCY_MAX_EVENTS_PER_BLOCK`]，尽快冲掉积压；
//! - **过期丢弃**：`age > deadline` 的 NoteOn 丢弃（时间已追不回，保其余事件
//!   的时间对齐）；NoteOff / 状态类事件永不丢（防挂音 / 上下文错乱）；
//! - **反馈闸（Governor）**：按 render-load EMA 与「时间已落后」证据分四级
//!   （Normal/High/Overload/Emergency）；Overload/Emergency 对本块**新鲜
//!   NoteOn** 施加准入预算（[`OVERLOAD_NOTE_ON_BUDGET`] /
//!   [`EMERGENCY_NOTE_ON_BUDGET`]），防止声部风暴持续注入；含迟滞与冷却，
//!   避免「丢弃→负载降→退出→再进入」的密度泵动。

use super::*;

/// 单块事件 drain 上限（正常路径）。
const MAX_EVENTS_PER_BLOCK: usize = 4_096;
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

/// 负载治理器：render-load EMA 平滑系数（与 CPU 侧 fork 同口径 α=0.2）。
const EMA_ALPHA: f64 = 0.2;
/// 升级到下一级所需的高负载持续块数。
const ENTER_SUSTAIN_BLOCKS: u32 = 20;
/// 降级（释放）所需的无压力持续块数（≈0.4s @ ~47 块/s）。
///
/// 释放不看负载 EMA，而看"压力"证据（见 [`Governor::observe`]）：风暴一停
/// 入口速率回落、丢弃消失即可逐级放行，避免"等引擎静下来才放行"的人工静音尾巴。
const RELEASE_SUSTAIN_BLOCKS: u32 = 20;
/// 级别进入阈值：Normal→High / High→Overload / Overload→Emergency。
const HIGH_ENTER_LOAD: f64 = 0.90;
const OVERLOAD_ENTER_LOAD: f64 = 1.20;
const EMERGENCY_ENTER_LOAD: f64 = 1.50;
/// Emergency 退出后的冷却块数（≈5s @ ~47 块/s），防密度泵动。
const EMERGENCY_COOLDOWN_BLOCKS: u32 = 235;
/// Overload 级的新鲜 NoteOn 单块准入预算。
///
/// 必须 ≤ 正常 drain 上限（[`MAX_EVENTS_PER_BLOCK`]），否则预算不会成为
/// 约束（每块最多处理后 4096 条）；预算的意义是让「新音符注入速率」低于
/// 「事件处理速率」，从而在过载时把复音增长压回可控范围（保其余事件对齐）。
const OVERLOAD_NOTE_ON_BUDGET: usize = 4_096;
/// Emergency 级的新鲜 NoteOn 单块准入预算（更严格：保节奏优先）。
const EMERGENCY_NOTE_ON_BUDGET: usize = 2_048;
/// Overload 级运行时声部上限（L4 软目标收缩，REND-016 #139）。
///
/// 块渲染成本 ∝ 声部数：过载时把 trim 目标临时压低，把渲染耗时拉回实时
/// 预算；恢复期由 Governor 逐级回升，风暴过后回到构造配置。
/// 取值依据（现场实测）：正常段落 ~3k 声部（load 0.1），爆点是**单 tick
/// 3 万声部**；上限必须低于爆点规模才能把块耗时压回预算，同时高于正常段落。
const OVERLOAD_VOICE_LIMIT: usize = 16_384;
/// Emergency 级运行时声部上限（保实时优先，比 Overload 更严格）。
const EMERGENCY_VOICE_LIMIT: usize = 8_192;
/// 声部数即时升级阈值（REND-016 #139）：声部超过此值时**跳过证据/EMA 等待**
/// 直接进 Emergency——保护必须在"声部爆掉的第一块"生效，否则单块耗时失控
/// （现场 1.4s 巨块 → 缓速）。
pub(crate) const VOICE_ESCALATE_VOICES: usize = 12_000;
// 编译期不变量：Emergency 必须比 Overload 更严格。
const _: () = assert!(OVERLOAD_VOICE_LIMIT > EMERGENCY_VOICE_LIMIT);

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

/// 治理级别（REND-016 #139）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GovernorLevel {
    /// 正常：无预算限制。
    Normal = 0,
    /// 偏高：仅观测（GPU 无运行时软目标，实际防护由硬上限 + 事件闸承担）。
    High = 1,
    /// 过载：新鲜 NoteOn 施加宽松预算。
    Overload = 2,
    /// 紧急：新鲜 NoteOn 施加严格预算 + 过期丢弃（事件闸）。
    Emergency = 3,
}

impl GovernorLevel {
    /// 本级对应的运行时声部上限（`None` = 恢复构造时配置）。
    ///
    /// 由渲染线程在级别切换时施加到 `GpuSynth::set_runtime_voice_limit`。
    pub(crate) fn voice_limit(self) -> Option<usize> {
        match self {
            GovernorLevel::Normal | GovernorLevel::High => None,
            GovernorLevel::Overload => Some(OVERLOAD_VOICE_LIMIT),
            GovernorLevel::Emergency => Some(EMERGENCY_VOICE_LIMIT),
        }
    }
}

/// 负载治理器状态机（见模块文档）。
pub(crate) struct Governor {
    level: GovernorLevel,
    ema: f64,
    ema_valid: bool,
    /// 连续高于「升级阈值」的块数。
    enter_streak: u32,
    /// 连续「无压力」的块数（释放依据）。
    release_streak: u32,
    /// Emergency 退出后的冷却剩余块数（>0 时禁止直接重入 Emergency）。
    cooldown: u32,
}

impl Governor {
    /// 创建治理器（Normal 起步）。
    pub(crate) fn new() -> Self {
        Self {
            level: GovernorLevel::Normal,
            ema: 0.0,
            ema_valid: false,
            enter_streak: 0,
            release_streak: 0,
            cooldown: 0,
        }
    }

    /// 当前级别。
    pub(crate) fn level(&self) -> GovernorLevel {
        self.level
    }

    /// 声部数超阈值的即时升级（REND-016 #139）：跳过证据/EMA 等待直入
    /// Emergency（冷却期也允许——声部爆掉的第一块就要 trim）。
    pub(crate) fn force_emergency(&mut self) -> GovernorLevel {
        self.set_level(GovernorLevel::Emergency)
    }

    /// 本块新鲜 NoteOn 的准入预算（`None` = 不限）。
    pub(crate) fn note_on_budget(&self) -> Option<usize> {
        match self.level {
            GovernorLevel::Normal | GovernorLevel::High => None,
            GovernorLevel::Overload => Some(OVERLOAD_NOTE_ON_BUDGET),
            GovernorLevel::Emergency => Some(EMERGENCY_NOTE_ON_BUDGET),
        }
    }

    /// 每块渲染完成后喂入负载、积压证据与丢弃压力；更新级别。
    ///
    /// 返回级别变化后的新级别（`Some` 表示发生切换，供调用方打点）。
    /// - `emergency_evidence`：本块事件闸触发了紧急冲洗（积压已深，允许越过
    ///   持续块数要求直接进入 Emergency；冷却期内除外）；
    /// - `gate_pressure`：本块仍有丢弃压力（发送端闸丢弃 / 过期丢弃 / 预算
    ///   丢弃 / 紧急冲洗）。**释放只看压力**：压力消失持续
    ///   [`RELEASE_SUSTAIN_BLOCKS`] 块即逐级放行——风暴一停就恢复，不等负载
    ///   EMA 慢慢回落（旧口径"等引擎静下来才放行"会制造爆点后的人工静音尾巴）。
    pub(crate) fn observe(
        &mut self,
        load: f64,
        emergency_evidence: bool,
        gate_pressure: bool,
    ) -> Option<GovernorLevel> {
        if self.cooldown > 0 {
            self.cooldown -= 1;
        }
        self.ema = if self.ema_valid {
            self.ema * (1.0 - EMA_ALPHA) + load * EMA_ALPHA
        } else {
            self.ema_valid = true;
            load
        };

        // 「时间已落后」证据：越过全部持续块数要求直入 Emergency（冷却期除外）。
        if emergency_evidence && self.cooldown == 0 && self.level != GovernorLevel::Emergency {
            return Some(self.set_level(GovernorLevel::Emergency));
        }

        // 升级：达到下一级阈值并持续足够块数。
        let next = match self.level {
            GovernorLevel::Normal => Some((GovernorLevel::High, HIGH_ENTER_LOAD)),
            GovernorLevel::High => Some((GovernorLevel::Overload, OVERLOAD_ENTER_LOAD)),
            GovernorLevel::Overload => Some((GovernorLevel::Emergency, EMERGENCY_ENTER_LOAD)),
            GovernorLevel::Emergency => None,
        };
        if let Some((next_level, threshold)) = next {
            if self.ema > threshold {
                self.enter_streak += 1;
            } else {
                self.enter_streak = 0;
            }
            if self.enter_streak >= ENTER_SUSTAIN_BLOCKS
                && (next_level != GovernorLevel::Emergency || self.cooldown == 0)
            {
                return Some(self.set_level(next_level));
            }
        } else {
            self.enter_streak = 0;
        }

        // 降级（释放）：看"压力"证据而不是负载 EMA——压力消失说明风暴已过
        // （入口速率回落），持续 RELEASE_SUSTAIN_BLOCKS 块后逐级回落。
        if gate_pressure {
            self.release_streak = 0;
        } else {
            self.release_streak += 1;
        }
        if self.release_streak >= RELEASE_SUSTAIN_BLOCKS && self.level != GovernorLevel::Normal {
            let was_emergency = self.level == GovernorLevel::Emergency;
            let new_level = match self.level {
                GovernorLevel::Emergency => GovernorLevel::Overload,
                GovernorLevel::Overload => GovernorLevel::High,
                GovernorLevel::High => GovernorLevel::Normal,
                GovernorLevel::Normal => GovernorLevel::Normal,
            };
            let changed = self.set_level(new_level);
            if was_emergency {
                self.cooldown = EMERGENCY_COOLDOWN_BLOCKS;
            }
            return Some(changed);
        }
        None
    }

    fn set_level(&mut self, level: GovernorLevel) -> GovernorLevel {
        self.level = level;
        self.enter_streak = 0;
        self.release_streak = 0;
        level
    }
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

/// 有界 drain 一帧积压事件（语义见模块文档）。
///
/// 准入预算来自 `governor`；`stats` 记录累计丢弃。
pub(crate) fn drain_events(
    rx: &mpsc::Receiver<StampedEvent>,
    now: Instant,
    deadline: Duration,
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

    while processed < cap {
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
        // 说明积压已深——本块一次性扫完过期区（而不是每块只扫 65536 条），
        // 让渲染线程立即回到新鲜区，消除"追平期间的长静音"。
        if processed == 1 && age > deadline * EMERGENCY_DRAIN_MULTIPLIER {
            cap = FLUSH_MAX_EVENTS;
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
        tracing::warn!(
            "[EVENT-FLUSH] 紧急冲洗：扫描 {} 条（NoteOff {} / 控制 {} / 过期 NoteOn {} / 预算 {}），最老 {}ms（累计 {}，L{}）",
            processed,
            note_offs,
            controls,
            dropped_expired,
            dropped_budget,
            oldest_ms,
            stats.dropped_note_ons() + dropped_expired,
            governor.level() as u8
        );
        eprintln!(
            "[EVENT-FLUSH] 紧急冲洗：扫描 {} 条（NoteOff {} / 控制 {} / 过期 NoteOn {} / 预算 {}），最老 {}ms（累计 {}，L{}）",
            processed,
            note_offs,
            controls,
            dropped_expired,
            dropped_budget,
            oldest_ms,
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
