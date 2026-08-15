//! 流式 MIDI 解析器——零事件常驻，逐事件按 tick 互锁多轨输出。
//!
//! 与 [MidiDocument]（全量加载、随机访问）不同，本模块为一次性顺序消费设计：
//!
//! - 基于 `midly::mmap` 零拷贝，不分配任何事件数据
//! - 每轨仅保持一个 `MmapEventIter` + 一个预读 peek 事件
//! - 多轨自动按 tick 互锁交织，正确处理 MIDI Format 1/2
//! - 适用于音频导出、流式传输等只需顺序访问的场景
//!
//! # 用法
//!
//! ```rust,ignore
//! use lumino_midi_loader::StreamingMidiPlayer;
//!
//! let bytes = std::fs::read("song.mid")?;
//! let mut player = StreamingMidiPlayer::from_bytes(&bytes)?;
//!
//! while let Some((tick, track_idx, kind)) = player.next_event() {
//!     println!("tick={}, track={}, event={:?}", tick, track_idx, kind);
//! }
//! ```

use midly::mmap::{MmapEventIter, MmapSmf, MmapTrack};
use midly::{MetaMessage, TrackEvent, TrackEventKind};

use crate::{LoaderError, LoaderResult};

// ── 轨道游标 ──────────────────────────────────────────────

/// 每轨前进游标。
///
/// 维护一个零拷贝迭代器 + 预读的下一个事件。
struct TrackCursor<'a> {
    iter: MmapEventIter<'a>,
    current_tick: u64,
    peeked_delta: u32,
    peeked_event: Option<Result<TrackEvent<'a>, midly::Error>>,
    exhausted: bool,
}

impl<'a> TrackCursor<'a> {
    fn new(track: &MmapTrack<'a>) -> Self {
        Self {
            iter: track.iter(),
            current_tick: 0,
            peeked_delta: 0,
            peeked_event: None,
            exhausted: false,
        }
    }

    /// 确保 `peeked_event` 有值。轨道耗尽时设 `exhausted = true`。
    fn ensure_peeked(&mut self) {
        if self.exhausted || self.peeked_event.is_some() {
            return;
        }
        match self.iter.next() {
            Some(Ok(ev)) => {
                self.peeked_delta = u32::from(ev.delta);
                self.peeked_event = Some(Ok(ev));
            }
            Some(Err(e)) => self.peeked_event = Some(Err(e)),
            None => self.exhausted = true,
        }
    }

    /// 下一个事件的绝对 tick。耗尽时返回 `u64::MAX`。
    fn next_tick(&self) -> u64 {
        if self.exhausted || self.peeked_event.is_none() {
            u64::MAX
        } else {
            self.current_tick + self.peeked_delta as u64
        }
    }

    /// 消费当前 peek 事件并预读下一个。
    /// 返回 `(delta, TrackEventKind)`。
    fn consume(&mut self) -> Option<Result<(u32, TrackEventKind<'a>), midly::Error>> {
        let ev = self.peeked_event.take()?;
        match ev {
            Ok(e) => {
                let delta = u32::from(e.delta);
                let kind = e.kind;
                self.current_tick += delta as u64;
                self.ensure_peeked();
                Some(Ok((delta, kind)))
            }
            Err(err) => {
                self.ensure_peeked();
                Some(Err(err))
            }
        }
    }
}

// ── Tempo 扫描结果 ────────────────────────────────────────

/// 预扫描结果：Tempo 变化列表 + 最大 tick。
struct ScanResult {
    tempo_changes: Vec<(u32, f32)>,
    total_ticks: u64,
    ppqn: u32,
}

/// 预扫描所有轨道的 Tempo 事件并累计最大 tick。
fn scan_tempos(smf: &MmapSmf) -> ScanResult {
    let mut changes: Vec<(u32, f32)> = Vec::new();
    let mut max_tick: u64 = 0;
    let ppqn = match smf.header().timing {
        midly::Timing::Metrical(t) => u16::from(t) as u32,
        midly::Timing::Timecode(_, _) => 480,
    };

    for track in smf.tracks() {
        let mut tick: u64 = 0;
        for ev in track.iter().flatten() {
            tick += u32::from(ev.delta) as u64;
            if let TrackEventKind::Meta(MetaMessage::Tempo(tempo)) = ev.kind {
                let bpm = 60_000_000.0 / tempo.as_int() as f32;
                if bpm > 0.0 {
                    changes.push((tick as u32, bpm));
                }
            }
        }
        max_tick = max_tick.max(tick);
    }

    // 确保至少有一个有效的起始速度
    if !changes.iter().any(|(t, _)| *t == 0) {
        changes.push((0, 120.0));
    }
    changes.sort_by_key(|a| a.0);
    changes.dedup_by(|a, b| {
        if a.0 == b.0 {
            core::mem::swap(a, b);
            true
        } else {
            false
        }
    });

    ScanResult {
        tempo_changes: changes,
        total_ticks: max_tick,
        ppqn,
    }
}

// ── StreamingMidiPlayer ───────────────────────────────────

/// 流式 MIDI 播放器——零事件常驻，逐事件按 tick 互锁输出。
///
/// 创建后通过 [`next_event`] 逐次获取事件，事件按全局 tick 升序排列。
/// 渲染完成后返回 `None`。
pub struct StreamingMidiPlayer<'a> {
    /// 保持字节数据的生命周期，`TrackCursor` 均借用至此。
    _data: core::marker::PhantomData<&'a [u8]>,
    /// MmapSmf 持有轨道字节切片引用。字段本身不需读取，但必须存活以维持借用。
    #[allow(dead_code)]
    mmap_smf: MmapSmf<'a>,
    tracks: Vec<TrackCursor<'a>>,
    /// 预扫描的 Tempo 变化（tick, BPM）
    pub tempo_changes: Vec<(u32, f32)>,
    /// 最大 tick
    pub total_ticks: u64,
    /// PPQN
    pub ppqn: u32,
}

impl<'a> StreamingMidiPlayer<'a> {
    /// 从 MIDI 文件字节创建流式播放器。
    ///
    /// 内部流程：
    /// 1. `MmapSmf::parse` 零拷贝解析头部 + 提取轨道字节切片
    /// 2. 预扫描所有轨道构建 TempoMap（仅扫描 Tempo meta 事件）
    /// 3. 创建每轨 TrackCursor 并预读第一轮
    pub fn from_bytes(data: &'a [u8]) -> LoaderResult<Self> {
        let mmap_smf = MmapSmf::parse(data)
            .map_err(|e| LoaderError::MidiParse(format!("MmapSmf 解析失败: {}", e)))?;

        let ScanResult {
            tempo_changes,
            total_ticks,
            ppqn,
        } = scan_tempos(&mmap_smf);

        let tracks: Vec<TrackCursor> = mmap_smf.tracks().iter().map(TrackCursor::new).collect();

        let mut player = Self {
            _data: core::marker::PhantomData,
            mmap_smf,
            tracks,
            tempo_changes,
            total_ticks,
            ppqn,
        };
        player.ensure_all_peeked();
        Ok(player)
    }

    /// 获取 MIDI 格式的 PPQN（每四分音符脉冲数）。
    #[inline]
    pub fn ppqn(&self) -> u32 {
        self.ppqn
    }

    /// 获取预扫描的 Tempo 变化列表。
    #[inline]
    pub fn tempo_changes(&self) -> &[(u32, f32)] {
        &self.tempo_changes
    }

    /// 获取总 tick 数。
    #[inline]
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    /// 获取轨道数量。
    #[inline]
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// 是否所有轨道均已耗尽。
    #[inline]
    pub fn is_exhausted(&self) -> bool {
        self.tracks.iter().all(|t| t.exhausted)
    }

    /// 获取下一个 MIDI 事件（按全局 tick 升序）。
    ///
    /// 返回 `(absolute_tick, track_index, TrackEventKind)`。
    /// 全部耗尽时返回 `None`。
    ///
    /// `TrackEventKind` 借用自原始数据，不受 `self` 后续调用的影响。
    pub fn next_event(&mut self) -> Option<(u64, usize, TrackEventKind<'a>)> {
        let (min_tick, ti) = self.find_min_tick_fast();
        if min_tick == u64::MAX {
            return None;
        }

        let consumed = self.tracks[ti].consume()?;

        match consumed {
            Ok((_delta, kind)) => Some((min_tick, ti, kind)),
            Err(e) => {
                // 解析错误：记录日志并跳过
                tracing::warn!("轨道 {} 事件解析错误: {}", ti, e);
                // 递归取下一个（跳过坏事件）
                self.next_event()
            }
        }
    }

    /// 确保所有轨道已预读第一个事件。
    fn ensure_all_peeked(&mut self) {
        for track in &mut self.tracks {
            track.ensure_peeked();
        }
    }

    /// 找到当前最小 tick 及对应的轨道索引（无分配版本）。
    /// 返回 (min_tick, first_track_index)。若有多个同 tick 轨道，后续 next_event 会取到。
    fn find_min_tick_fast(&self) -> (u64, usize) {
        let mut min_tick = u64::MAX;
        let mut min_track = 0;
        for (i, track) in self.tracks.iter().enumerate() {
            let nt = track.next_tick();
            if nt < min_tick {
                min_tick = nt;
                min_track = i;
            }
        }
        (min_tick, min_track)
    }
}

// ── 测试 ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MIDI_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-file/test_note_worker_bench_assets/Erosoul.mid",
    );

    // ── 测试数据自举 ──────────────────────────────────────
    //
    // `test-file/` 目录被 .gitignore 忽略（本地压测资产，不入库），CI 克隆后文件不存在。
    // 因此测试优先读取本地真实 MIDI；缺失时回退到 [generated_test_midi] 运行时生成的
    // 合法 SMF，保证任意环境（CI/新克隆）都能跑通且结果确定。

    /// 加载测试 MIDI：本地真实文件存在则用之，否则用生成的自举数据。
    fn load_test_midi() -> Vec<u8> {
        std::fs::read(TEST_MIDI_PATH).unwrap_or_else(|_| generated_test_midi())
    }

    /// 将事件字节序列封装为 MTrk 块并追加到 `out`。
    fn push_track(out: &mut Vec<u8>, events: &[u8]) {
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(events.len() as u32).to_be_bytes());
        out.extend_from_slice(events);
    }

    /// MIDI 变长数量（VLQ）编码。
    fn vlq(mut n: u32) -> Vec<u8> {
        let mut bytes = vec![(n & 0x7F) as u8];
        n >>= 7;
        while n > 0 {
            bytes.push(((n & 0x7F) as u8) | 0x80);
            n >>= 7;
        }
        bytes.reverse();
        bytes
    }

    /// 生成确定性合法的标准 MIDI 文件（Format 1，3 轨，480 PPQN）：
    /// - 轨 0（conductor）：tick 0 处 120 BPM、tick 960 处 90 BPM 两次速度变化
    /// - 轨 1：3 个音符（key 60/64/64，ch 0），起音 tick 0/480/960
    /// - 轨 2：1 个音符（key 64，ch 1），起音 tick 240（与轨 1 交织）
    fn generated_test_midi() -> Vec<u8> {
        let mut out = Vec::new();

        // 头块 MThd：Format 1、3 轨、480 PPQN
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&6u32.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&3u16.to_be_bytes());
        out.extend_from_slice(&480u16.to_be_bytes());

        // 轨 0：速度变化 + 轨结束
        let mut track0 = vlq(0);
        track0.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 120 BPM
        track0.extend_from_slice(&vlq(960));
        track0.extend_from_slice(&[0xFF, 0x51, 0x03, 0x0A, 0x2C, 0x2A]); // 90 BPM
        track0.extend_from_slice(&vlq(0));
        track0.extend_from_slice(&[0xFF, 0x2F, 0x00]); // End of Track
        push_track(&mut out, &track0);

        // 轨 1：3 个音符（起音 tick 0/480/960）
        let mut track1 = vlq(0);
        track1.extend_from_slice(&[0x90, 60, 100]); // NoteOn ch0 key60 vel100
        track1.extend_from_slice(&vlq(480));
        track1.extend_from_slice(&[0x80, 60, 64]); // NoteOff ch0 key60
        track1.extend_from_slice(&vlq(480));
        track1.extend_from_slice(&[0x90, 64, 100]); // NoteOn ch0 key64
        track1.extend_from_slice(&vlq(480));
        track1.extend_from_slice(&[0x80, 64, 64]); // NoteOff ch0 key64
        track1.extend_from_slice(&vlq(0));
        track1.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        push_track(&mut out, &track1);

        // 轨 2：1 个音符（起音 tick 240，与轨 1 交织）
        let mut track2 = vlq(240);
        track2.extend_from_slice(&[0x91, 64, 80]); // NoteOn ch1 key64 vel80
        track2.extend_from_slice(&vlq(240));
        track2.extend_from_slice(&[0x81, 64, 64]); // NoteOff ch1 key64
        track2.extend_from_slice(&vlq(0));
        track2.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        push_track(&mut out, &track2);

        out
    }

    /// 验证 `StreamingMidiPlayer` 能正确解析（本地真实文件优先，缺失时回退生成数据）
    /// 并逐事件输出。
    #[test]
    fn test_real_midi_parses() {
        let file_bytes = load_test_midi();
        let mut player =
            StreamingMidiPlayer::from_bytes(&file_bytes).expect("real MIDI file should parse");

        assert!(player.ppqn > 0, "PPQN should be positive");
        assert!(player.total_ticks > 0, "total ticks should be positive");
        assert!(player.track_count() > 0, "track count should be positive");
        assert!(
            !player.tempo_changes.is_empty(),
            "should have tempo changes"
        );

        // 逐事件遍历——验证不 panic
        let mut event_count = 0u64;
        while let Some((tick, _track, _kind)) = player.next_event() {
            event_count += 1;
            // tick 应该非递减（但同一 tick 可有多个事件）
            assert!(
                tick <= player.total_ticks,
                "tick {} should not exceed {}",
                tick,
                player.total_ticks
            );
        }
        assert!(event_count > 0, "should have at least one event");
        assert!(player.is_exhausted(), "player should be exhausted");
    }

    /// 验证 `next_event()` 返回的事件 tick 按非递减顺序排列。
    #[test]
    fn test_events_in_order() {
        let file_bytes = load_test_midi();
        let mut player =
            StreamingMidiPlayer::from_bytes(&file_bytes).expect("real MIDI file should parse");

        let mut prev_tick: u64 = 0;
        while let Some((tick, _track, _kind)) = player.next_event() {
            assert!(
                tick >= prev_tick,
                "event tick {} should be >= previous tick {}",
                tick,
                prev_tick
            );
            prev_tick = tick;
        }
    }

    /// 验证多轨事件互锁——events 来自至少 2 个不同的音轨。
    ///
    /// 这是对逐轨串行 bug 的回归测试：如果 `next_event()` 只输出 track 0 的事件，
    /// 则 `distinct_tracks` 集合中只会有一个元素。此测试确保多轨 MIDI 的每一轨
    /// 事件都被正确交织输出。
    #[test]
    fn test_multi_track_interleave() {
        let file_bytes = load_test_midi();
        let mut player =
            StreamingMidiPlayer::from_bytes(&file_bytes).expect("real MIDI file should parse");

        let track_count = player.track_count();
        assert!(
            track_count >= 2,
            "test MIDI should have at least 2 tracks for multi-track test"
        );

        let mut distinct_tracks: std::collections::BTreeSet<usize> =
            std::collections::BTreeSet::new();
        while let Some((_tick, track_idx, kind)) = player.next_event() {
            // 只统计有意义的 MIDI 事件（忽略 Meta 事件所在的 conductor track）
            if matches!(kind, TrackEventKind::Midi { .. }) {
                distinct_tracks.insert(track_idx);
            }
        }

        assert!(
            distinct_tracks.len() >= 2,
            "MIDI events should come from at least 2 tracks, got {}: {:?}",
            distinct_tracks.len(),
            distinct_tracks,
        );
    }

    /// 验证 `NoteOn` 事件正确产生 `(key, vel)` 参数。
    #[test]
    fn test_note_on_events() {
        let file_bytes = load_test_midi();
        let mut player =
            StreamingMidiPlayer::from_bytes(&file_bytes).expect("real MIDI file should parse");

        let mut note_count = 0u64;
        while let Some((_tick, _track, kind)) = player.next_event() {
            if let TrackEventKind::Midi {
                message: midly::MidiMessage::NoteOn { key: _, vel: _ },
                ..
            } = kind
            {
                note_count += 1;
            }
        }
        assert!(note_count > 0, "should have at least one NoteOn event");
    }

    /// 生成数据的自足性回归测试：不依赖任何外部文件，确保 CI 上生成器产出合法 SMF。
    #[test]
    fn test_generated_midi_self_sufficient() {
        let bytes = generated_test_midi();
        let mut player = StreamingMidiPlayer::from_bytes(&bytes).expect("生成 MIDI 应可解析");

        assert_eq!(player.ppqn, 480, "PPQN 应为 480");
        assert_eq!(player.track_count(), 3, "应有 3 个轨道");
        assert!(player.total_ticks > 0, "total ticks should be positive");
        assert!(
            !player.tempo_changes.is_empty(),
            "should have tempo changes"
        );

        let mut note_count = 0u64;
        let mut distinct_tracks: std::collections::BTreeSet<usize> =
            std::collections::BTreeSet::new();
        let mut prev_tick = 0u64;
        while let Some((tick, track_idx, kind)) = player.next_event() {
            assert!(tick >= prev_tick, "tick 应非递减: {tick} < {prev_tick}");
            prev_tick = tick;
            if matches!(kind, TrackEventKind::Midi { .. }) {
                distinct_tracks.insert(track_idx);
            }
            if let TrackEventKind::Midi {
                message: midly::MidiMessage::NoteOn { .. },
                ..
            } = kind
            {
                note_count += 1;
            }
        }
        assert!(player.is_exhausted(), "player should be exhausted");
        assert!(note_count >= 3, "至少应有 3 个 NoteOn，got {note_count}");
        assert!(
            distinct_tracks.len() >= 2,
            "MIDI 事件应来自至少 2 个轨道，got {:?}",
            distinct_tracks,
        );
    }
}
