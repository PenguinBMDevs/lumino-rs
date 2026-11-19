//! REND-016 #139 事件积压治理（有界 drain / 过期丢弃 / 准入预算）单元测试。
//!
//! 自 `drain.rs` 拆出（文件行数守卫 <400）；负载治理器用例见 `tests/governor.rs`。

use super::governor::EMERGENCY_NOTE_ON_BUDGET;
use super::*;

mod governor;

/// 计数闸 / 紧急冲洗 / 准入预算测试用的时间预算：大到墙钟永不成为约束。
///
/// 这些用例断言的是**事件条数**语义，与墙钟无关；若沿用真实预算（1024@64k
/// 只有 4.8ms），CI 慢机一次调度抢占就能让时间闸在 256 条处先触发，断言随
/// 负载抖动（macOS runner 实测 `left: 256, right: 4096`）。预算本身随块周期
/// 的映射由 [`test_drain_time_budget_scales_with_block_period`] 单独覆盖。
const TEST_DRAIN_BUDGET: Duration = Duration::from_secs(60);

fn test_stats() -> PlaybackStatsReader {
    PlaybackStatsReader {
        samples: Arc::new(AtomicI64::new(0)),
        last_request_samples: Arc::new(AtomicI64::new(0)),
        last_samples_after_read: Arc::new(AtomicI64::new(0)),
        render_time: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
        render_time_head: Arc::new(AtomicU64::new(0)),
        render_size: Arc::new(AtomicU64::new(0)),
        samples_per_second: 96_000,
        voice_count: Arc::new(AtomicU64::new(0)),
        underruns: Arc::new(AtomicU64::new(0)),
        playing: Arc::new(AtomicBool::new(false)),
        dropped_note_ons: Arc::new(AtomicU64::new(0)),
        governor_level: Arc::new(AtomicU64::new(0)),
        sample_gpu_bytes: Arc::new(AtomicU64::new(0)),
        sample_cpu_bytes: Arc::new(AtomicU64::new(0)),
        sample_resample_bytes: Arc::new(AtomicU64::new(0)),
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

/// 时间预算 = 块周期 × [`DRAIN_DUTY`]，夹在 [4ms, 32ms]（块大时随周期放大）。
#[test]
fn test_drain_time_budget_scales_with_block_period() {
    // 8ms 块（512@64k）：2.4ms → 下限 4ms（块极小时保住最低吞吐）。
    assert_eq!(
        drain_time_budget(Duration::from_millis(8)),
        DRAIN_TIME_BUDGET_MIN
    );
    // 16ms 块（1024@64k）：4.8ms。
    let mid = drain_time_budget(Duration::from_millis(16));
    assert!(
        mid >= Duration::from_micros(4_700) && mid <= Duration::from_micros(4_900),
        "16ms 块预算应约 4.8ms，实际 {mid:?}"
    );
    // 100ms 块：30ms，仍在封顶内。
    let wide = drain_time_budget(Duration::from_millis(100));
    assert!(
        wide >= Duration::from_micros(29_500) && wide <= Duration::from_micros(30_500),
        "100ms 块预算应约 30ms，实际 {wide:?}"
    );
    // 128ms 块（8192@64k）：38.4ms → 上限 32ms。
    assert_eq!(
        drain_time_budget(Duration::from_millis(128)),
        DRAIN_TIME_BUDGET_MAX
    );
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
        Duration::from_millis(16),
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
    let first = drain_events_with_budget(
        &rx,
        now,
        Duration::from_millis(50),
        TEST_DRAIN_BUDGET,
        &governor,
        &stats,
        |_, _| n += 1,
    );
    assert_eq!(
        first.processed, MAX_EVENTS_PER_BLOCK,
        "单块处理量受正常上限约束"
    );
    assert_eq!(n, MAX_EVENTS_PER_BLOCK);

    let second = drain_events_with_budget(
        &rx,
        now,
        Duration::from_millis(50),
        TEST_DRAIN_BUDGET,
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
    let outcome = drain_events_with_budget(
        &rx,
        now,
        Duration::from_millis(50),
        TEST_DRAIN_BUDGET,
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
    governor.observe(0.5, true, false); // 证据直入 Emergency
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
    let outcome = drain_events_with_budget(
        &rx,
        now,
        Duration::from_millis(50),
        TEST_DRAIN_BUDGET,
        &governor,
        &stats,
        |_, _| delivered += 1,
    );
    assert_eq!(outcome.dropped_budget, 10, "超预算的新鲜 NoteOn 被丢");
    assert_eq!(outcome.dropped_expired, 0);
    assert_eq!(delivered, EMERGENCY_NOTE_ON_BUDGET + 1, "NoteOff 必须保留");
    assert_eq!(stats.dropped_note_ons(), 10);
}

/// 紧急冲洗：积压深时**一次调用**横扫整个过期区（时间预算内，远超旧 65536
/// 上限），立即回到新鲜区——消除"追平期间数秒级静音"（现场断续问题的根因）。
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
    let outcome = drain_events_with_budget(
        &rx,
        now,
        Duration::from_millis(50),
        // 断言的是"一次调用横扫整个过期区"（条数语义），时间预算取大值以
        // 排除墙钟抖动；真实预算下的分块横扫由计数闸单独保证。
        TEST_DRAIN_BUDGET,
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
