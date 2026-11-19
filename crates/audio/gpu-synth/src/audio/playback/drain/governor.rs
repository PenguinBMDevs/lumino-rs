//! REND-016 #139：负载治理器（render-load EMA + 积压证据 → 四级状态机）。
//!
//! 从 `drain.rs` 拆出（文件行数守卫 <400）。级别决定本块**新鲜 NoteOn** 的
//! 准入预算与运行时声部上限；含升级持续块数、释放迟滞与 Emergency 冷却，
//! 防「丢弃→负载降→退出→再进入」的密度泵动。

use std::time::Duration;

/// 负载治理器：render-load EMA 平滑系数（与 CPU 侧 fork 同口径 α=0.2）。
const EMA_ALPHA: f64 = 0.2;
/// 升级到下一级所需的高负载持续块数。
pub(crate) const ENTER_SUSTAIN_BLOCKS: u32 = 20;
/// 降级（释放）所需的无压力持续块数（≈0.4s @ ~47 块/s）。
///
/// 释放不看负载 EMA，而看"压力"证据（见 [`Governor::observe`]）：风暴一停
/// 入口速率回落、丢弃消失即可逐级放行，避免"等引擎静下来才放行"的人工静音尾巴。
pub(crate) const RELEASE_SUSTAIN_BLOCKS: u32 = 20;
/// 级别进入阈值：Normal→High / High→Overload / Overload→Emergency。
const HIGH_ENTER_LOAD: f64 = 0.90;
pub(crate) const OVERLOAD_ENTER_LOAD: f64 = 1.20;
const EMERGENCY_ENTER_LOAD: f64 = 1.50;
/// Emergency 退出后的冷却块数（≈5s @ ~47 块/s），防密度泵动。
pub(crate) const EMERGENCY_COOLDOWN_BLOCKS: u32 = 235;
/// Overload 级的新鲜 NoteOn 单块准入预算。
///
/// 必须 ≤ 正常 drain 上限（`MAX_EVENTS_PER_BLOCK`），否则预算不会成为约束
/// （每块最多处理后 4096 条）；预算的意义是让「新音符注入速率」低于
/// 「事件处理速率」，从而在过载时把复音增长压回可控范围（保其余事件对齐）。
pub(crate) const OVERLOAD_NOTE_ON_BUDGET: usize = 4_096;
/// Emergency 级的新鲜 NoteOn 单块准入预算（更严格：保节奏优先）。
pub(crate) const EMERGENCY_NOTE_ON_BUDGET: usize = 2_048;
/// Overload 级运行时声部上限**封顶**（L4 软目标收缩，REND-016 #139）。
///
/// 实际值由 [`voice_limits_for`] 按块周期缩放（块小 → 每块可负担的声部少）。
/// 取值依据（现场实测）：正常段落 ~3k 声部（load 0.1），爆点是**单 tick
/// 3 万声部**；上限必须低于爆点规模才能把块耗时压回预算，同时高于正常段落。
pub(crate) const OVERLOAD_VOICE_LIMIT: usize = 16_384;
/// Emergency 级运行时声部上限**封顶**（保实时优先，比 Overload 更严格）。
pub(crate) const EMERGENCY_VOICE_LIMIT: usize = 8_192;
/// 运行时声部上限下限（块极小时防止过度收缩）。
pub(crate) const OVERLOAD_VOICE_LIMIT_FLOOR: usize = 4_096;
pub(crate) const EMERGENCY_VOICE_LIMIT_FLOOR: usize = 2_048;
// 编译期不变量：Emergency 必须比 Overload 更严格。
const _: () = assert!(OVERLOAD_VOICE_LIMIT > EMERGENCY_VOICE_LIMIT);

/// 按块周期推导运行时声部上限（REND-016 #139）。
///
/// 每声部每块成本 ≈1µs（现场 12288 声部 ≈12.5ms/块，`collect`+`upload`
/// 主导，与块大小无关）——**块越小，每块可负担的声部越少**。固定
/// 8192（+50% 淡出槽 = 12288）在 512 块（8ms 预算）下必然饱和（音频断续，
/// 现场"最后一段卡过去"）。Overload 允许占块预算 60%，Emergency 40%
/// （更严，另留 4ms 给 drain 下限与调度）；受构造常量封顶、下限保底。
pub(crate) fn voice_limits_for(block_period: Duration) -> (usize, usize) {
    let period_us = block_period.as_micros() as usize;
    let overload = (period_us * 6 / 10).clamp(OVERLOAD_VOICE_LIMIT_FLOOR, OVERLOAD_VOICE_LIMIT);
    let emergency = (period_us * 4 / 10).clamp(EMERGENCY_VOICE_LIMIT_FLOOR, EMERGENCY_VOICE_LIMIT);
    (overload, emergency)
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
    /// REND-016 #139：按块周期缩放的运行时声部上限（[`voice_limits_for`]）。
    overload_voice_limit: usize,
    emergency_voice_limit: usize,
}

impl Governor {
    /// 创建治理器（Normal 起步；声部上限先用构造常量，启动时由
    /// [`Governor::set_voice_limits`] 按块周期覆盖）。
    pub(crate) fn new() -> Self {
        Self {
            level: GovernorLevel::Normal,
            ema: 0.0,
            ema_valid: false,
            enter_streak: 0,
            release_streak: 0,
            cooldown: 0,
            overload_voice_limit: OVERLOAD_VOICE_LIMIT,
            emergency_voice_limit: EMERGENCY_VOICE_LIMIT,
        }
    }

    /// 设置按块周期缩放的运行时声部上限（启动时调用一次）。
    pub(crate) fn set_voice_limits(&mut self, overload: usize, emergency: usize) {
        self.overload_voice_limit = overload;
        self.emergency_voice_limit = emergency;
    }

    /// 当前级别对应的运行时声部上限（`None` = 恢复构造时配置）。
    pub(crate) fn voice_limit(&self) -> Option<usize> {
        match self.level {
            GovernorLevel::Normal | GovernorLevel::High => None,
            GovernorLevel::Overload => Some(self.overload_voice_limit),
            GovernorLevel::Emergency => Some(self.emergency_voice_limit),
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
