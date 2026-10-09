//! REND-016 #139：渲染线程的事件积压治理（有界 drain + 过期 NoteOn 丢弃）。
//!
//! GPU 特有的杀手是「事件积压一次性注入」：大块渲染结束后把积压的上万事件
//! 全部 drain 进下一块 → 复音更高 → 块更大 → 级联，最终缓冲队列耗尽（欠载
//! 无声）。CPU 侧有 per-channel drain cap 天然免疫；本模块给 GPU 补上双闸中
//! 的事件闸：
//!
//! - **有界 drain**：每块最多 [`MAX_EVENTS_PER_BLOCK`] 条（其余留待后续块）；
//! - **过期丢弃**：`age > deadline` 的 NoteOn 丢弃（时间已追不回，保其余事件
//!   的时间对齐）；NoteOff / 状态类事件永不丢（防挂音 / 上下文错乱）；
//! - **紧急放宽**：最老事件年龄超过 [`EMERGENCY_DRAIN_MULTIPLIER`]×deadline 时
//!   升到 [`EMERGENCY_MAX_EVENTS_PER_BLOCK`]，尽快冲掉积压。

use super::*;

/// 单块事件 drain 上限（正常路径）。
const MAX_EVENTS_PER_BLOCK: usize = 4_096;
/// 紧急路径：最老事件年龄超过该倍数 deadline 时放宽单块上限。
const EMERGENCY_DRAIN_MULTIPLIER: u32 = 4;
/// 紧急路径单块 drain 上限。
const EMERGENCY_MAX_EVENTS_PER_BLOCK: usize = 65_536;
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

/// 有界 drain 一帧积压事件（语义见模块文档）。
///
/// 返回本块实际处理（含丢弃）的事件数，供上层需要时观测。
pub(crate) fn drain_events(
    rx: &mpsc::Receiver<StampedEvent>,
    now: Instant,
    deadline: Duration,
    stats: &PlaybackStatsReader,
    mut on_event: impl FnMut(u8, MidiEvent),
) -> usize {
    let mut cap = MAX_EVENTS_PER_BLOCK;
    let mut processed = 0usize;
    let mut dropped = 0u64;

    while processed < cap {
        let Ok((channel, event, enqueued_at)) = rx.try_recv() else {
            break;
        };
        processed += 1;
        let age = now.saturating_duration_since(enqueued_at);

        // 紧急放宽：FIFO 下首个事件即最老事件，其年龄超限说明积压已深。
        if processed == 1 && age > deadline * EMERGENCY_DRAIN_MULTIPLIER {
            cap = EMERGENCY_MAX_EVENTS_PER_BLOCK;
        }

        if age > deadline && is_droppable(&event) {
            dropped += 1;
            continue;
        }
        on_event(channel, event);
    }

    if dropped > 0 {
        stats.record_dropped_note_ons(dropped);
        log_drop_rate_limited(dropped, stats);
    }
    processed
}

/// 限频（500ms）输出丢弃告警；与 `[UNDERRUN]` 同款 stderr 标记，便于现场诊断。
fn log_drop_rate_limited(dropped: u64, stats: &PlaybackStatsReader) {
    static LAST_DROP_LOG: AtomicI64 = AtomicI64::new(0);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let prev = LAST_DROP_LOG.fetch_max(ms, Ordering::Relaxed);
    if ms - prev > 500 {
        eprintln!(
            "[EVENT-DROP] 过期 NoteOn 丢弃 {} 条（累计 {}）",
            dropped,
            stats.dropped_note_ons()
        );
    }
}

#[cfg(test)]
mod tests {
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
        let processed = drain_events(&rx, now, Duration::from_millis(50), &stats, |ch, ev| {
            got.push((ch, ev));
        });

        assert_eq!(processed, 5, "处理计数包含被丢弃的事件");
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
        let mut n = 0usize;
        let first = drain_events(&rx, now, Duration::from_millis(50), &stats, |_, _| n += 1);
        assert_eq!(first, MAX_EVENTS_PER_BLOCK, "单块处理量受正常上限约束");
        assert_eq!(n, MAX_EVENTS_PER_BLOCK);

        let second = drain_events(&rx, now, Duration::from_millis(50), &stats, |_, _| n += 1);
        assert_eq!(second, 10, "剩余事件留待后续块");
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
        let processed = drain_events(&rx, now, Duration::from_millis(50), &stats, |_, _| {});
        assert_eq!(processed, count, "紧急路径应突破正常上限");
        assert_eq!(
            stats.dropped_note_ons() as usize,
            count,
            "全部过期 NoteOn 被丢弃"
        );
    }
}
