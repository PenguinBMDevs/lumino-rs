//! REND-016 #139 事件积压治理与负载治理器单元测试。
//!
//! 自 `drain.rs` 拆出（文件行数守卫 <400）。

use super::*;

fn test_stats() -> PlaybackStatsReader {
    PlaybackStatsReader {
        samples: Arc::new(AtomicI64::new(0)),
        last_request_samples: Arc::new(AtomicI64::new(0)),
        last_samples_after_read: Arc::new(AtomicI64::new(0)),
        render_time: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
        render_time_head: Arc::new(AtomicU64::new(0)),
        render_size: Arc::new(AtomicU64::new(0)),
        voice_count: Arc::new(AtomicU64::new(0)),
        underruns: Arc::new(AtomicU64::new(0)),
        dropped_note_ons: Arc::new(AtomicU64::new(0)),
        governor_level: Arc::new(AtomicU64::new(0)),
    }
}

/// deadline = 3×块时长，且不小于 50ms 下限。
#[test]
fn test_event_deadline_scale_and_floor() {
    // 1024 @ 48k：块长 ≈21.3ms → 3× ≈64ms
    let d = event_deadline(1024, 48_000);
    assert!(
        d >= Duration::from_millis(63) && d <= Duration::from_millis(65),
        "1024@48k 的 deadline 应约为 64ms，实际 {d:?}"
    );
    // 16 @ 64k：块长 ≈0.25ms → 3× ≈0.75ms → 下限 50ms
    assert_eq!(event_deadline(16, 64_000), Duration::from_millis(50));
}

/// 正常负载下治理器保持 Normal，且无预算限制。
#[test]
fn test_governor_stays_normal_under_normal_load() {
    let mut g = Governor::new();
    for _ in 0..300 {
        assert_eq!(g.observe(0.5, false), None);
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
        g.observe(0.95, false);
    }
    assert_eq!(g.level(), GovernorLevel::Normal, "未满持续块数不得升级");
    for _ in 0..50 {
        g.observe(0.95, false);
    }
    assert_eq!(g.level(), GovernorLevel::High);
    assert_eq!(g.note_on_budget(), None, "High 级不限制预算");

    // 1.3 > OVERLOAD_ENTER（阈值 1.2），持续累计后进入 Overload
    for _ in 0..60 {
        g.observe(1.3, false);
    }
    assert_eq!(g.level(), GovernorLevel::Overload);
    assert_eq!(g.note_on_budget(), Some(OVERLOAD_NOTE_ON_BUDGET));

    // 1.6 > EMERGENCY_ENTER（阈值 1.5），持续累计后进入 Emergency
    for _ in 0..60 {
        g.observe(1.6, false);
    }
    assert_eq!(g.level(), GovernorLevel::Emergency);
    assert_eq!(g.note_on_budget(), Some(EMERGENCY_NOTE_ON_BUDGET));
}

/// 「时间已落后」证据可越过持续块数要求，直接进入 Emergency。
#[test]
fn test_governor_evidence_enters_emergency_immediately() {
    let mut g = Governor::new();
    let change = g.observe(0.5, true);
    assert_eq!(change, Some(GovernorLevel::Emergency));
    assert_eq!(g.note_on_budget(), Some(EMERGENCY_NOTE_ON_BUDGET));
}

/// 降级需恢复持续 100 块、逐级回落；Emergency 退出后有冷却期。
#[test]
fn test_governor_exit_hysteresis_and_cooldown() {
    let mut g = Governor::new();
    g.observe(0.5, true); // 证据直入 Emergency
    assert_eq!(g.level(), GovernorLevel::Emergency);

    // 恢复期：0.5 < RECOVER_LOAD，100 块后回落到 Overload
    for _ in 0..(RECOVER_SUSTAIN_BLOCKS - 1) {
        g.observe(0.5, false);
    }
    assert_eq!(g.level(), GovernorLevel::Emergency, "未满恢复块数不得降级");
    g.observe(0.5, false);
    assert_eq!(g.level(), GovernorLevel::Overload, "逐级回落");

    // 冷却期内：证据也不能直接重入 Emergency
    assert_eq!(g.observe(0.5, true), None, "冷却期内禁止证据直入");
    assert_eq!(g.level(), GovernorLevel::Overload);

    // 冷却剩余 234；喂高负载至冷却剩 9 块时仍被门控
    for _ in 0..(EMERGENCY_COOLDOWN_BLOCKS - 10) {
        g.observe(1.6, false);
    }
    assert_eq!(g.level(), GovernorLevel::Overload, "冷却未结束不得升级");

    // 再喂 10 块 → 冷却归零，负载持续满足 → 升级 Emergency
    for _ in 0..10 {
        g.observe(1.6, false);
    }
    assert_eq!(g.level(), GovernorLevel::Emergency, "冷却结束应可再次升级");
}

/// 只有过期 NoteOn 被丢；NoteOff / CC / PC 无论多老都保留；新鲜事件全保留。
#[test]
fn test_drain_drops_only_expired_note_on() {
    let (tx, rx) = mpsc::channel::<StampedEvent>();
    let now = Instant::now();
    let old = now
        .checked_sub(Duration::from_millis(500))
        .expect("时钟回绕");
    tx.send((0, MidiEvent::NoteOn { key: 1, vel: 100 }, old))
        .expect("send");
    tx.send((0, MidiEvent::NoteOff { key: 1 }, old))
        .expect("send");
    tx.send((
        0,
        MidiEvent::ControlChange {
            controller: 64,
            value: 0,
        },
        old,
    ))
    .expect("send");
    tx.send((0, MidiEvent::NoteOn { key: 2, vel: 100 }, now))
        .expect("send");
    tx.send((1, MidiEvent::ProgramChange { program: 3 }, old))
        .expect("send");

    let stats = test_stats();
    let mut got: Vec<(u8, MidiEvent)> = Vec::new();
    let outcome = drain_events(
        &rx,
        now,
        Duration::from_millis(50),
        &Governor::new(),
        &stats,
        |ch, ev| got.push((ch, ev)),
    );

    assert_eq!(outcome.processed, 5, "处理计数包含被丢弃的事件");
    assert_eq!(outcome.dropped_expired, 1);
    assert_eq!(outcome.dropped_budget, 0);
    assert_eq!(got.len(), 4, "仅过期的 NoteOn 被丢弃");
    assert!(matches!(got[0].1, MidiEvent::NoteOff { key: 1 }));
    assert!(matches!(
        got[1].1,
        MidiEvent::ControlChange { controller: 64, .. }
    ));
    assert!(matches!(got[2].1, MidiEvent::NoteOn { key: 2, .. }));
    assert!(matches!(got[3].1, MidiEvent::ProgramChange { program: 3 }));
    assert_eq!(stats.dropped_note_ons(), 1);
}

/// 正常路径：单块最多 `MAX_EVENTS_PER_BLOCK` 条，其余留待下块（不丢）。
#[test]
fn test_drain_cap_defers_excess() {
    let (tx, rx) = mpsc::channel::<StampedEvent>();
    let now = Instant::now();
    for i in 0..(MAX_EVENTS_PER_BLOCK + 10) {
        tx.send((
            0,
            MidiEvent::NoteOn {
                key: (i % 128) as u8,
                vel: 1,
            },
            now,
        ))
        .expect("send");
    }

    let stats = test_stats();
    let governor = Governor::new();
    let mut n = 0usize;
    let first = drain_events(
        &rx,
        now,
        Duration::from_millis(50),
        &governor,
        &stats,
        |_, _| n += 1,
    );
    assert_eq!(
        first.processed, MAX_EVENTS_PER_BLOCK,
        "单块处理量受正常上限约束"
    );
    assert_eq!(n, MAX_EVENTS_PER_BLOCK);

    let second = drain_events(
        &rx,
        now,
        Duration::from_millis(50),
        &governor,
        &stats,
        |_, _| n += 1,
    );
    assert_eq!(second.processed, 10, "剩余事件留待后续块");
    assert_eq!(n, MAX_EVENTS_PER_BLOCK + 10);
    assert_eq!(stats.dropped_note_ons(), 0, "新鲜事件不得被丢");
}

/// 紧急放宽：最老事件超 4×deadline 时，单块处理量突破正常上限。
#[test]
fn test_drain_emergency_escalates_cap() {
    let (tx, rx) = mpsc::channel::<StampedEvent>();
    let now = Instant::now();
    let very_old = now
        .checked_sub(Duration::from_millis(1000))
        .expect("时钟回绕");
    let count = MAX_EVENTS_PER_BLOCK + 1;
    for i in 0..count {
        tx.send((
            0,
            MidiEvent::NoteOn {
                key: (i % 128) as u8,
                vel: 1,
            },
            very_old,
        ))
        .expect("send");
    }

    let stats = test_stats();
    let outcome = drain_events(
        &rx,
        now,
        Duration::from_millis(50),
        &Governor::new(),
        &stats,
        |_, _| {},
    );
    assert_eq!(outcome.processed, count, "紧急路径应突破正常上限");
    assert!(outcome.emergency_evidence, "应报告积压证据");
    assert_eq!(
        stats.dropped_note_ons() as usize,
        count,
        "全部过期 NoteOn 被丢弃"
    );
}

/// Emergency 级的准入预算：新鲜 NoteOn 超预算被丢，NoteOff 不受影响。
#[test]
fn test_drain_budget_drops_fresh_note_ons_in_emergency() {
    let mut governor = Governor::new();
    governor.observe(0.5, true); // 证据直入 Emergency
    assert_eq!(governor.level(), GovernorLevel::Emergency);

    let (tx, rx) = mpsc::channel::<StampedEvent>();
    let now = Instant::now();
    let count = EMERGENCY_NOTE_ON_BUDGET + 10;
    for i in 0..count {
        tx.send((
            0,
            MidiEvent::NoteOn {
                key: (i % 128) as u8,
                vel: 1,
            },
            now,
        ))
        .expect("send");
    }
    tx.send((0, MidiEvent::NoteOff { key: 0 }, now))
        .expect("send");

    let stats = test_stats();
    let mut delivered = 0usize;
    let outcome = drain_events(
        &rx,
        now,
        Duration::from_millis(50),
        &governor,
        &stats,
        |_, _| delivered += 1,
    );
    assert_eq!(outcome.dropped_budget, 10, "超预算的新鲜 NoteOn 被丢");
    assert_eq!(outcome.dropped_expired, 0);
    assert_eq!(delivered, EMERGENCY_NOTE_ON_BUDGET + 1, "NoteOff 必须保留");
    assert_eq!(stats.dropped_note_ons(), 10);
}

/// REND-016 #139：Governor 等级 → 运行时声部上限映射（L4 软目标收缩）。
#[test]
fn test_governor_level_maps_to_voice_limit() {
    assert_eq!(GovernorLevel::Normal.voice_limit(), None);
    assert_eq!(GovernorLevel::High.voice_limit(), None);
    assert_eq!(
        GovernorLevel::Overload.voice_limit(),
        Some(OVERLOAD_VOICE_LIMIT)
    );
    assert_eq!(
        GovernorLevel::Emergency.voice_limit(),
        Some(EMERGENCY_VOICE_LIMIT)
    );
}

/// 紧急冲洗：积压深时**一次调用**横扫整个过期区（远超旧 65536 上限），
/// 立即回到新鲜区——消除"追平期间数秒级静音"（现场断续问题的根因）。
#[test]
fn test_drain_flushes_deep_backlog_in_one_pass() {
    let (tx, rx) = mpsc::channel::<StampedEvent>();
    let now = Instant::now();
    let old = now
        .checked_sub(Duration::from_millis(1000))
        .expect("时钟回绕");
    let expired = 65_536 + 10; // 超过历史"紧急放宽"上限（65536），证明一次性冲刷

    for i in 0..expired {
        tx.send((
            0,
            MidiEvent::NoteOn {
                key: (i % 128) as u8,
                vel: 1,
            },
            old,
        ))
        .expect("send");
    }
    tx.send((0, MidiEvent::NoteOff { key: 0 }, old))
        .expect("send"); // 陈旧但不可丢
    tx.send((0, MidiEvent::NoteOn { key: 60, vel: 100 }, now))
        .expect("send"); // 新鲜区

    let stats = test_stats();
    let mut delivered = 0usize;
    let outcome = drain_events(
        &rx,
        now,
        Duration::from_millis(50),
        &Governor::new(),
        &stats,
        |_, _| delivered += 1,
    );

    assert!(outcome.emergency_evidence, "应报告积压证据");
    assert_eq!(
        outcome.processed,
        expired + 2,
        "一次调用应横扫整个过期区（超过旧紧急上限）"
    );
    assert_eq!(outcome.dropped_expired as usize, expired);
    assert_eq!(delivered, 2, "陈旧 NoteOff 与新鲜 NoteOn 必须投递");
    assert_eq!(stats.dropped_note_ons() as usize, expired);
}
