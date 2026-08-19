//! 流式播放器本体与 tempo 预扫描（从 `streaming.rs` 拆分）

use midly::mmap::MmapSmf;
use midly::{MetaMessage, TrackEventKind};

use crate::{LoaderError, LoaderResult};

use super::cursor::TrackCursor;

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
/// 创建后通过 `next_event` 逐次获取事件，事件按全局 tick 升序排列。
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
