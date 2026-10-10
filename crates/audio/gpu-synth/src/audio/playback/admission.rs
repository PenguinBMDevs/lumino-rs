//! REND-016 #139：发送端 NoteOn 准入限速（GPU 版软 NPS 闸，Governor 自动驱动）。
//!
//! 黑 MIDI 洪峰实测可达 ~1.8M 事件/s，远超管线（drain/引擎）处理上限；渲染端
//! 冲洗只能清掉"已积压"的事件，无法阻止下一块周期又涌入几十万条超龄事件 →
//! 表现为周期性的"出声一下 → 自然衰减 → 静音"。
//!
//! 本闸在**发送端**限速 NoteOn（NoteOff / 状态类事件全放行），策略与 CPU
//! （xsynth-Lumino `EmergencyGate`）对齐：**令牌桶 + 突发容量**——
//! 桶容量 = 50ms 的音符量，桶满时一批和弦/重音**成组通过**，持续超量才逐个
//! 拒绝；被拒 NoteOn 的 NoteOff 会被**配对抵消**（防挂音，顺带减少事件量）。
//!
//! 速率由渲染线程的 Governor 级别驱动：
//! - Normal / High：不限速（正常素材零影响）；
//! - Overload：≈ [`OVERLOAD_NPS`] 音符/s；Emergency：≈ [`EMERGENCY_NPS`] 音符/s；
//! - 用户开关（LGS 防爆闸，默认开）关闭时恒放行、零丢音路径。

use super::*;

use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};

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
/// 突发容量：50ms 的音符量（CPU 为 100ms，这里略收紧以减少块耗时尖峰）。
const BURST_MS: i64 = 50;
/// 每个音符的毫令牌数（定点，1 音符 = 1000 毫令牌）。
const TOKENS_PER_NOTE: i64 = 1_000;
/// (channel, key) 配对表大小（全局通道 256 × 键 128）。
const PAIR_SLOTS: usize = 256 * 128;

/// 级别 → 准入速率（0 = 不限速）。
fn rate_for_level(level: u64) -> i64 {
    if level >= LEVEL_EMERGENCY {
        EMERGENCY_NPS
    } else if level >= LEVEL_OVERLOAD {
        OVERLOAD_NPS
    } else {
        0
    }
}

/// 令牌桶内部状态（毫令牌定点数）。
struct BucketState {
    tokens_milli: i64,
    burst_milli: i64,
    last_us: i64,
}

/// 发送端准入状态（渲染线程发布级别，发送端应用令牌桶限速）。
pub(crate) struct AdmissionState {
    /// 用户开关（LGS 防爆闸；false = 恒放行）。
    enabled: AtomicBool,
    /// 当前 Governor 级别（渲染线程每块发布；0 = Normal）。
    level: AtomicU64,
    /// 令牌桶。
    bucket: Mutex<BucketState>,
    /// 被闸丢弃的 NoteOn 累计数。
    dropped: AtomicU64,
    /// 被丢弃 NoteOn 的配对计数（(ch,key) → 待抵消的 NoteOff 数）。
    skipped: Box<[AtomicU32]>,
}

impl AdmissionState {
    /// 创建（`enabled` = 用户开关；默认 Normal = 不限速）。
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            level: AtomicU64::new(0),
            bucket: Mutex::new(BucketState {
                tokens_milli: 0,
                burst_milli: 0,
                last_us: 0,
            }),
            dropped: AtomicU64::new(0),
            skipped: (0..PAIR_SLOTS).map(|_| AtomicU32::new(0)).collect(),
        }
    }

    /// 渲染线程发布当前治理级别。
    ///
    /// 级别变化时重置令牌桶为满突发（与 CPU `EmergencyGate::set` 同语义），
    /// 避免携带旧的空桶导致启用瞬间完全静音。
    pub(crate) fn set_level(&self, level: u64) {
        let old = self.level.swap(level, Ordering::Relaxed);
        if old != level {
            let rate = rate_for_level(level);
            let mut b = self.bucket.lock().unwrap_or_else(|e| e.into_inner());
            b.burst_milli = rate.max(0).saturating_mul(BURST_MS);
            b.tokens_milli = b.burst_milli;
            b.last_us = now_us();
        }
    }

    /// 用户开关状态。
    pub(crate) fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// 当前级别。
    pub(crate) fn level(&self) -> u64 {
        self.level.load(Ordering::Relaxed)
    }

    /// 被闸丢弃的 NoteOn 累计数。
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// 按给定时刻判定是否放行一个 NoteOn（纯令牌桶，不含配对；便于单测）。
    ///
    /// 未启用 / Normal / High 恒放行；Overload / Emergency 下：桶满时一批
    /// 成组通过，持续超量则按速率补充（µs 粒度）。
    pub(crate) fn allow_note_on_at(&self, now_us: i64) -> bool {
        if !self.enabled() {
            return true;
        }
        let rate = rate_for_level(self.level());
        if rate <= 0 {
            return true;
        }
        let mut b = self.bucket.lock().unwrap_or_else(|e| e.into_inner());
        let elapsed = now_us.saturating_sub(b.last_us);
        if elapsed > 0 {
            b.tokens_milli = (b.tokens_milli + elapsed * rate / 1000).min(b.burst_milli);
            b.last_us = now_us;
        }
        if b.tokens_milli >= TOKENS_PER_NOTE {
            b.tokens_milli -= TOKENS_PER_NOTE;
            true
        } else {
            false
        }
    }

    /// 真实时钟放行判定 + 丢弃计数 + 配对记录 + 限频告警。
    pub(crate) fn note_on_allowed(&self, channel: u8, key: u8) -> bool {
        let allowed = self.allow_note_on_at(now_us());
        if allowed {
            return true;
        }
        self.dropped.fetch_add(1, Ordering::Relaxed);
        let slot = channel as usize * 128 + key as usize;
        if let Some(counter) = self.skipped.get(slot) {
            let _ = counter.fetch_add(1, Ordering::Relaxed);
        }
        self.log_gate_rate_limited();
        false
    }

    /// NoteOff（或 vel=0 的 NoteOn）配对抵消。
    ///
    /// 若对应 NoteOn 曾被丢弃，则吞掉该 NoteOff（返回 `true`），避免向引擎
    /// 发送永远不会发声的 NoteOff（CPU 同语义）。
    pub(crate) fn note_off_paired(&self, channel: u8, key: u8) -> bool {
        let slot = channel as usize * 128 + key as usize;
        let Some(counter) = self.skipped.get(slot) else {
            return false;
        };
        let mut cur = counter.load(Ordering::Relaxed);
        while cur > 0 {
            match counter.compare_exchange_weak(cur, cur - 1, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(actual) => cur = actual,
            }
        }
        false
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
/// Overload/Emergency 下受令牌桶限速（成组放行）；被拒 NoteOn 的 NoteOff
/// 被配对吞掉；其余事件全放行。
#[derive(Clone)]
pub struct EventSender {
    tx: mpsc::Sender<StampedEvent>,
    admission: Arc<AdmissionState>,
}

impl EventSender {
    /// 构造一个独立注入器（自带准入状态；测试/工具用；默认启用闸）。
    pub fn new(tx: mpsc::Sender<StampedEvent>) -> Self {
        Self {
            tx,
            admission: Arc::new(AdmissionState::new(true)),
        }
    }

    /// 构造共享治理器准入状态的注入器（`AudioPlayback::event_sender` 用）。
    pub(crate) fn shared(tx: mpsc::Sender<StampedEvent>, admission: Arc<AdmissionState>) -> Self {
        Self { tx, admission }
    }

    /// 发送一个 MIDI 事件（NoteOn 受限速闸约束；其余全放行）。
    pub fn send(&self, channel: u8, event: MidiEvent) {
        // 闸判定：true = 本次事件被拦截（NoteOn 限速丢弃 / NoteOff 配对抵消）。
        let gated_out = match &event {
            MidiEvent::NoteOn { key, vel } if *vel > 0 => {
                !self.admission.note_on_allowed(channel, *key)
            }
            MidiEvent::NoteOff { key } => self.admission.note_off_paired(channel, *key),
            // vel == 0 的 NoteOn 与 NoteOff 同语义（配对抵消）。
            MidiEvent::NoteOn { key, .. } => self.admission.note_off_paired(channel, *key),
            _ => false,
        };
        if gated_out {
            return;
        }
        let _ = self.tx.send((channel, event, Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 开关关闭 / Normal / High：恒放行（正常素材零影响）。
    #[test]
    fn gate_is_open_when_disabled_or_below_overload() {
        let disabled = AdmissionState::new(false);
        disabled.set_level(3);
        for _ in 0..10_000 {
            assert!(disabled.allow_note_on_at(0), "关闭开关时不得限速");
        }

        let s = AdmissionState::new(true);
        for i in 0..100 {
            assert!(s.allow_note_on_at(i), "Normal 下不得限速");
        }
        s.set_level(1); // High
        for i in 0..100 {
            assert!(s.allow_note_on_at(i));
        }
        assert_eq!(s.dropped(), 0);
    }

    /// Overload：50ms 突发成组放行，之后按 32k/s 补充。
    #[test]
    fn gate_admits_burst_then_enforces_rate() {
        let s = AdmissionState::new(true);
        s.set_level(2);
        let burst_notes = OVERLOAD_NPS * BURST_MS / 1000; // 1600
        for i in 0..burst_notes {
            assert!(s.allow_note_on_at(1_000_000), "突发内第 {i} 个应放行");
        }
        assert!(!s.allow_note_on_at(1_000_000), "突发耗尽后应拒绝");
        // 1ms 补充 32 个（32k/s）
        let mut allowed = 0;
        for _ in 0..100 {
            if s.allow_note_on_at(1_001_000) {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 32, "1ms 应恰好补充 32 个");
    }

    /// Emergency 更严格：突发 400，补充 8/ms。
    #[test]
    fn gate_emergency_is_stricter() {
        let s = AdmissionState::new(true);
        s.set_level(3);
        let burst_notes = EMERGENCY_NPS * BURST_MS / 1000; // 400
        for _ in 0..burst_notes {
            assert!(s.allow_note_on_at(1_000_000));
        }
        assert!(!s.allow_note_on_at(1_000_000));
        let mut allowed = 0;
        for _ in 0..100 {
            if s.allow_note_on_at(1_001_000) {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 8);
    }

    /// 级别变化时令牌桶重置为满突发（避免携带空桶）。
    #[test]
    fn level_change_resets_bucket() {
        let s = AdmissionState::new(true);
        s.set_level(3);
        let burst_notes = EMERGENCY_NPS * BURST_MS / 1000;
        for _ in 0..burst_notes {
            assert!(s.allow_note_on_at(1_000_000));
        }
        assert!(!s.allow_note_on_at(1_000_000), "桶已空");
        s.set_level(2); // 升级恢复：重置为 Overload 满桶
        assert!(s.allow_note_on_at(1_000_000), "级别变化应重置满桶");
    }

    /// 配对抵消：被丢 NoteOn 的 NoteOff 被吞掉，未配对的 NoteOff 正常放行。
    #[test]
    fn skipped_note_on_pairs_its_note_off() {
        let s = AdmissionState::new(true);
        s.set_level(3);
        // 清空桶且不随时间补充 → 下一个 NoteOn 必被丢（确定性）
        {
            let mut b = s.bucket.lock().unwrap_or_else(|e| e.into_inner());
            b.tokens_milli = 0;
            b.last_us = i64::MAX;
        }
        assert!(!s.note_on_allowed(0, 60), "桶空时 NoteOn 应被丢");
        assert!(s.note_off_paired(0, 60), "对应 NoteOff 应被吞掉");
        assert!(!s.note_off_paired(0, 60), "只有一个待配对");
        assert!(!s.note_off_paired(0, 61), "未丢过的键不受影响");
        assert_eq!(s.dropped(), 1);
    }

    /// 发送器端到端：仅真实 NoteOn 受闸；配对的 NoteOff 被吞；其余全放行。
    #[test]
    fn event_sender_gates_note_on_and_swallows_paired_note_off() {
        let (tx, rx) = mpsc::channel::<StampedEvent>();
        let sender = EventSender::new(tx);
        sender.admission.set_level(3);
        {
            let mut b = sender
                .admission
                .bucket
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            b.tokens_milli = 0;
            b.last_us = i64::MAX;
        }

        sender.send(0, MidiEvent::NoteOn { key: 60, vel: 100 }); // 被丢
        sender.send(0, MidiEvent::NoteOff { key: 60 }); // 配对吞掉
        sender.send(0, MidiEvent::NoteOff { key: 61 }); // 放行
        sender.send(
            0,
            MidiEvent::ControlChange {
                controller: 64,
                value: 0,
            },
        ); // 放行
        sender.send(0, MidiEvent::NoteOn { key: 62, vel: 0 }); // vel=0 放行

        let mut got = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            got.push(ev.1);
        }
        assert_eq!(got.len(), 3, "被丢 NoteOn 与其配对 NoteOff 均不入队");
        assert!(matches!(got[0], MidiEvent::NoteOff { key: 61 }));
        assert!(matches!(
            got[1],
            MidiEvent::ControlChange { controller: 64, .. }
        ));
        assert!(matches!(got[2], MidiEvent::NoteOn { key: 62, vel: 0 }));
    }
}
