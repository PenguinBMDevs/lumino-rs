//! REND-016 #139：发送端 NoteOn 准入限速（GPU 版软 NPS 闸，Governor 自动驱动）。
//!
//! 黑 MIDI 洪峰实测可达 ~1.8M 事件/s，远超管线（drain/引擎）处理上限；渲染端
//! 冲洗只能清掉"已积压"的事件，无法阻止下一块周期又涌入几十万条超龄事件 →
//! 表现为周期性的"出声一下 → 自然衰减 → 静音"。本闸在**发送端**把 NoteOn
//! 限速成细流（NoteOff / 状态类事件全放行），使渲染端始终面对可按时处理的
//! 事件量。速率由渲染线程的 Governor 级别驱动：
//!
//! - Normal / High：不限速（正常素材零影响）；
//! - Overload：≈ [`OVERLOAD_NPS`] 音符/s；
//! - Emergency：≈ [`EMERGENCY_NPS`] 音符/s（保连续优先）。

use super::*;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// 进程内单调时钟（微秒）。
fn now_us() -> i64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    Instant::now().duration_since(*epoch).as_micros() as i64
}

/// Overload 级 NoteOn 准入速率（音符/s）。
const OVERLOAD_NPS: i64 = 32_000;
/// Emergency 级 NoteOn 准入速率（音符/s）。
const EMERGENCY_NPS: i64 = 8_000;
/// Governor 级别阈值（数值与 `drain::GovernorLevel` 对齐）。
const LEVEL_OVERLOAD: u64 = 2;
const LEVEL_EMERGENCY: u64 = 3;
/// 首次放行用的哨兵：`now - MIN/2` 恒为巨大值。
const LAST_SENTINEL: i64 = i64::MIN / 2;

/// 发送端准入状态（渲染线程发布级别，发送端应用限速）。
pub(crate) struct AdmissionState {
    /// 当前 Governor 级别（渲染线程每块发布；0 = Normal）。
    level: AtomicU64,
    /// 上一次放行的 NoteOn 时刻（微秒）。
    last_note_on_us: AtomicI64,
    /// 被闸丢弃的 NoteOn 累计数。
    dropped: AtomicU64,
}

impl AdmissionState {
    /// 创建（默认 Normal = 不限速）。
    pub(crate) fn new() -> Self {
        Self {
            level: AtomicU64::new(0),
            last_note_on_us: AtomicI64::new(LAST_SENTINEL),
            dropped: AtomicU64::new(0),
        }
    }

    /// 渲染线程发布当前治理级别。
    pub(crate) fn set_level(&self, level: u64) {
        self.level.store(level, Ordering::Relaxed);
    }

    /// 当前级别。
    pub(crate) fn level(&self) -> u64 {
        self.level.load(Ordering::Relaxed)
    }

    /// 被闸丢弃的 NoteOn 累计数。
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// 按给定时刻判定是否放行一个 NoteOn（纯时间参数，便于单测）。
    ///
    /// Normal/High 恒放行；Overload/Emergency 按最小间隔（1/rate 秒）放行，
    /// 间隔不足则丢弃并计数。时间戳用 CAS 推进，多发送端共享同一状态。
    pub(crate) fn allow_note_on_at(&self, now_us: i64) -> bool {
        let rate = match self.level() {
            l if l >= LEVEL_EMERGENCY => EMERGENCY_NPS,
            l if l >= LEVEL_OVERLOAD => OVERLOAD_NPS,
            _ => return true,
        };
        let min_gap = 1_000_000 / rate;
        let mut last = self.last_note_on_us.load(Ordering::Relaxed);
        loop {
            if now_us.saturating_sub(last) < min_gap {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return false;
            }
            match self.last_note_on_us.compare_exchange_weak(
                last,
                now_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(current) => last = current,
            }
        }
    }

    /// 放行判定（真实时钟）+ 限频 `[NPS-GATE]` 告警。
    pub(crate) fn allow_note_on(&self) -> bool {
        let allowed = self.allow_note_on_at(now_us());
        if !allowed {
            self.log_gate_rate_limited();
        }
        allowed
    }

    fn log_gate_rate_limited(&self) {
        static LAST_GATE_LOG: AtomicI64 = AtomicI64::new(0);
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let prev = LAST_GATE_LOG.fetch_max(ms, Ordering::Relaxed);
        if ms - prev > 500 {
            tracing::warn!(
                "[NPS-GATE] 发送端限速丢弃 NoteOn 累计 {}（L{}）",
                self.dropped(),
                self.level()
            );
            eprintln!(
                "[NPS-GATE] 发送端限速丢弃 NoteOn 累计 {}（L{}）",
                self.dropped(),
                self.level()
            );
        }
    }
}

/// REND-016 #139：实时事件注入器（mpsc 发送端 + 发送端准入闸）。
///
/// 所有 MIDI 事件经此进入渲染线程：入队时盖墙钟时间戳；NoteOn（vel>0）在
/// Overload/Emergency 下受限速闸约束，NoteOff / 状态类事件永不限流。
#[derive(Clone)]
pub struct EventSender {
    tx: mpsc::Sender<StampedEvent>,
    admission: Arc<AdmissionState>,
}

impl EventSender {
    /// 构造一个独立注入器（自带未连接治理器的准入状态；测试/工具用）。
    pub fn new(tx: mpsc::Sender<StampedEvent>) -> Self {
        Self {
            tx,
            admission: Arc::new(AdmissionState::new()),
        }
    }

    /// 构造共享治理器准入状态的注入器（`AudioPlayback::event_sender` 用）。
    pub(crate) fn shared(tx: mpsc::Sender<StampedEvent>, admission: Arc<AdmissionState>) -> Self {
        Self { tx, admission }
    }

    /// 发送一个 MIDI 事件（NoteOn 受限速闸约束；其余全放行）。
    pub fn send(&self, channel: u8, event: MidiEvent) {
        if let MidiEvent::NoteOn { vel, .. } = &event
            && *vel > 0
            && !self.admission.allow_note_on()
        {
            return;
        }
        let _ = self.tx.send((channel, event, Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Normal/High 级别不限速（正常素材零影响）。
    #[test]
    fn gate_is_open_below_overload() {
        let s = AdmissionState::new();
        for i in 0..100 {
            assert!(s.allow_note_on_at(i), "Normal 下不得限速");
        }
        s.set_level(1); // High
        for i in 0..100 {
            assert!(s.allow_note_on_at(i));
        }
        assert_eq!(s.dropped(), 0);
    }

    /// Overload：最小间隔 31us（32k/s）；间隔不足则丢弃并计数。
    #[test]
    fn gate_enforces_overload_rate() {
        let s = AdmissionState::new();
        s.set_level(2);
        assert!(s.allow_note_on_at(1_000_000));
        assert!(!s.allow_note_on_at(1_000_010), "31us 内应被限");
        assert!(s.allow_note_on_at(1_000_040), "超过间隔应放行");
        assert_eq!(s.dropped(), 1);
    }

    /// Emergency：最小间隔 125us（8k/s），比 Overload 更严格。
    #[test]
    fn gate_emergency_is_stricter() {
        let s = AdmissionState::new();
        s.set_level(3);
        assert!(s.allow_note_on_at(1_000_000));
        assert!(!s.allow_note_on_at(1_000_100));
        assert!(s.allow_note_on_at(1_000_130));
        // 同样的时间差在 Overload 下会放行，证明 Emergency 更严格
        let o = AdmissionState::new();
        o.set_level(2);
        assert!(o.allow_note_on_at(1_000_000));
        assert!(o.allow_note_on_at(1_000_100));
    }

    /// 发送器：Emergency 下 NoteOn 受闸，NoteOff / vel=0 / 状态事件全放行。
    #[test]
    fn event_sender_gates_only_real_note_on() {
        let (tx, rx) = mpsc::channel::<StampedEvent>();
        let sender = EventSender::new(tx);
        sender.admission.set_level(3);

        // 把上次放行时刻推到未来 → 下一个 NoteOn 必被限（确定性，不依赖真实时钟）
        sender
            .admission
            .last_note_on_us
            .store(i64::MAX, Ordering::Relaxed);
        sender.send(0, MidiEvent::NoteOn { key: 60, vel: 100 });
        sender.send(0, MidiEvent::NoteOff { key: 60 });
        sender.send(0, MidiEvent::NoteOn { key: 61, vel: 0 });
        sender.send(
            0,
            MidiEvent::ControlChange {
                controller: 64,
                value: 0,
            },
        );

        let mut got = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            got.push(ev.1);
        }
        assert_eq!(got.len(), 3, "仅真实 NoteOn 被限速丢弃");
        assert!(matches!(got[0], MidiEvent::NoteOff { key: 60 }));
        assert!(matches!(got[1], MidiEvent::NoteOn { key: 61, vel: 0 }));
        assert!(matches!(
            got[2],
            MidiEvent::ControlChange { controller: 64, .. }
        ));
        assert_eq!(sender.admission.dropped(), 1);
    }

    /// 闸恢复：级别回落到 Normal 后立即全放行。
    #[test]
    fn gate_reopens_after_recovery() {
        let s = AdmissionState::new();
        s.set_level(3);
        assert!(s.allow_note_on_at(1_000_000));
        assert!(!s.allow_note_on_at(1_000_000));
        s.set_level(0);
        assert!(s.allow_note_on_at(1_000_000));
    }
}
