//! REND-016 #139 负载治理器单元测试。
//!
//! 自 `tests.rs` 拆出（文件行数守卫 <400）。

use std::time::Duration;

use super::super::governor::*;

/// 正常负载下治理器保持 Normal，且无预算限制。
#[test]
fn test_governor_stays_normal_under_normal_load() {
    let mut g = Governor::new();
    for _ in 0..300 {
        assert_eq!(g.observe(0.5, false, false), None);
    }
    assert_eq!(g.level(), GovernorLevel::Normal);
    assert_eq!(g.note_on_budget(), None);
}

/// 高负载持续累计后逐级升级：High → Overload → Emergency，各级预算正确。
#[test]
fn test_governor_escalates_on_sustained_load() {
    let mut g = Governor::new();
    // 0.95 > HIGH_ENTER
    for _ in 0..10 {
        g.observe(0.95, false, false);
    }
    assert_eq!(g.level(), GovernorLevel::Normal, "未满持续块数不得升级");
    for _ in 0..50 {
        g.observe(0.95, false, false);
    }
    assert_eq!(g.level(), GovernorLevel::High);
    assert_eq!(g.note_on_budget(), None, "High 级不限制预算");

    // 1.3 > OVERLOAD_ENTER（阈值 1.2），持续累计后进入 Overload
    for _ in 0..60 {
        g.observe(1.3, false, false);
    }
    assert_eq!(g.level(), GovernorLevel::Overload);
    assert_eq!(g.note_on_budget(), Some(OVERLOAD_NOTE_ON_BUDGET));

    // 1.6 > EMERGENCY_ENTER（阈值 1.5），持续累计后进入 Emergency
    for _ in 0..60 {
        g.observe(1.6, false, false);
    }
    assert_eq!(g.level(), GovernorLevel::Emergency);
    assert_eq!(g.note_on_budget(), Some(EMERGENCY_NOTE_ON_BUDGET));
}

/// 「时间已落后」证据可越过持续块数要求，直接进入 Emergency。
#[test]
fn test_governor_evidence_enters_emergency_immediately() {
    let mut g = Governor::new();
    let change = g.observe(0.5, true, false);
    assert_eq!(change, Some(GovernorLevel::Emergency));
    assert_eq!(g.note_on_budget(), Some(EMERGENCY_NOTE_ON_BUDGET));
}

/// 释放看「压力」：压力消失持续 `RELEASE_SUSTAIN_BLOCKS` 块即逐级回落；
/// Emergency 退出后有冷却期（期间证据也不能直入）。
#[test]
fn test_governor_release_on_no_pressure_and_cooldown() {
    let mut g = Governor::new();
    g.observe(0.5, true, false); // 证据直入 Emergency
    assert_eq!(g.level(), GovernorLevel::Emergency);

    // 无压力但未满释放块数：不得降级
    for _ in 0..(RELEASE_SUSTAIN_BLOCKS - 1) {
        g.observe(0.5, false, false);
    }
    assert_eq!(g.level(), GovernorLevel::Emergency, "未满释放块数不得降级");
    g.observe(0.5, false, false);
    assert_eq!(g.level(), GovernorLevel::Overload, "无压力满额后逐级回落");

    // 冷却期内：证据也不能直接重入 Emergency
    assert_eq!(g.observe(0.5, true, false), None, "冷却期内禁止证据直入");
    assert_eq!(g.level(), GovernorLevel::Overload);

    // 冷却剩余 234；带压力喂高负载至冷却剩 9 块 → 保持 Overload（冷却门控升级）
    for _ in 0..(EMERGENCY_COOLDOWN_BLOCKS - 10) {
        g.observe(1.6, false, true);
    }
    assert_eq!(g.level(), GovernorLevel::Overload, "冷却未结束不得升级");

    // 再喂 10 块 → 冷却归零 + 高负载持续 → 升级 Emergency
    for _ in 0..10 {
        g.observe(1.6, false, true);
    }
    assert_eq!(g.level(), GovernorLevel::Emergency, "冷却结束应可再次升级");
}

/// 丢弃压力持续存在时，即使负载 EMA 已回落也**不释放**（防止风暴未过就放行）。
#[test]
fn test_governor_pressure_blocks_release() {
    let mut g = Governor::new();
    g.observe(0.5, true, false);
    assert_eq!(g.level(), GovernorLevel::Emergency);
    for _ in 0..(RELEASE_SUSTAIN_BLOCKS * 5) {
        g.observe(0.5, false, true);
    }
    assert_eq!(g.level(), GovernorLevel::Emergency, "有压力时不得释放");
    for _ in 0..RELEASE_SUSTAIN_BLOCKS {
        g.observe(0.5, false, false);
    }
    assert_eq!(g.level(), GovernorLevel::Overload, "压力消失后应释放");
}

/// 声部数即时升级：`force_emergency` 跳过证据/EMA 等待直入 Emergency。
#[test]
fn test_governor_force_emergency_escalates_immediately() {
    let mut governor = Governor::new();
    assert_eq!(governor.level(), GovernorLevel::Normal);
    assert_eq!(governor.force_emergency(), GovernorLevel::Emergency);
    assert_eq!(governor.level(), GovernorLevel::Emergency);
    assert_eq!(
        governor.note_on_budget(),
        Some(EMERGENCY_NOTE_ON_BUDGET),
        "即时升级后应施加 Emergency 准入预算"
    );
}

/// REND-016 #139：Governor 等级 → 运行时声部上限映射（L4 软目标收缩，
/// 值随块周期缩放，见 [`voice_limits_for`]）。
#[test]
fn test_governor_level_maps_to_voice_limit() {
    let mut governor = Governor::new();
    assert_eq!(governor.voice_limit(), None, "Normal 不限制");

    // 按块周期缩放后的上限（512/1024 两档）。
    governor.set_voice_limits(4_800, 3_200);
    governor.force_emergency();
    assert_eq!(
        governor.voice_limit(),
        Some(3_200),
        "Emergency 用缩放后的上限（512 块 8ms 预算）"
    );

    // Overload：负载持续超阈值后使用 Overload 缩放上限。
    let mut g = Governor::new();
    g.set_voice_limits(4_800, 3_200);
    for _ in 0..(ENTER_SUSTAIN_BLOCKS * 2) {
        g.observe(OVERLOAD_ENTER_LOAD + 0.1, false, false);
    }
    assert_eq!(g.level(), GovernorLevel::Overload);
    assert_eq!(g.voice_limit(), Some(4_800));
}

/// 声部上限随块周期缩放（每声部每块 ≈1µs）：512/1024/8192 三档 + 下限保底。
#[test]
fn test_voice_limits_scale_with_block_period() {
    assert_eq!(
        voice_limits_for(Duration::from_millis(8)),
        (4_800, 3_200),
        "512@64k：8ms 预算下 Emergency 只负担 3200 声部"
    );
    assert_eq!(
        voice_limits_for(Duration::from_millis(16)),
        (9_600, 6_400),
        "1024@64k"
    );
    assert_eq!(
        voice_limits_for(Duration::from_millis(128)),
        (OVERLOAD_VOICE_LIMIT, EMERGENCY_VOICE_LIMIT),
        "8192 块：封顶"
    );
    let (o, e) = voice_limits_for(Duration::from_millis(2));
    assert!(
        o >= OVERLOAD_VOICE_LIMIT_FLOOR && e >= EMERGENCY_VOICE_LIMIT_FLOOR,
        "块极小时有下限保底"
    );
}
