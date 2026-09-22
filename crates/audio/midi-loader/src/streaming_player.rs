//! 流式播放器本体与 tempo 预扫描（从 `streaming.rs` 拆分）

use std::cmp::Reverse;
use std::collections::BinaryHeap;

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
    /// k 路归并最小堆：各轨当前事件 `(tick, track_idx)`，`Reverse` 使其成为小顶堆。
    ///
    /// 同一 tick 的多轨事件按 `track_idx` 升序输出（与旧"逐轨线性扫描取最小"
    /// 的稳定语义一致）。每轨最多一条堆内条目，消费后立即重新入堆。
    heap: BinaryHeap<Reverse<(u64, usize)>>,
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
            heap: BinaryHeap::new(),
            tempo_changes,
            total_ticks,
            ppqn,
        };
        player.ensure_all_peeked();
        player.rebuild_heap();
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
    ///
    /// 实现为 k 路归并（每轨游标 + 小顶堆）：旧实现每个事件都线性扫描全部轨道
    /// （O(轨道数)/事件），130 轨 × 3800 万事件下是数十亿次无效比较。
    pub fn next_event(&mut self) -> Option<(u64, usize, TrackEventKind<'a>)> {
        loop {
            let Reverse((min_tick, ti)) = self.heap.pop()?;
            let Some(consumed) = self.tracks[ti].consume() else {
                // 防御：堆内条目必然对应已预读事件；出现即状态异常，跳过并继续
                continue;
            };
            match consumed {
                Ok((_delta, kind)) => {
                    self.push_next_track(ti);
                    return Some((min_tick, ti, kind));
                }
                Err(e) => {
                    // 解析错误：记录日志并跳过（与旧行为一致）
                    tracing::warn!("轨道 {} 事件解析错误: {}", ti, e);
                    self.push_next_track(ti);
                }
            }
        }
    }

    /// 确保所有轨道已预读第一个事件。
    fn ensure_all_peeked(&mut self) {
        for track in &mut self.tracks {
            track.ensure_peeked();
        }
    }

    /// 重建归并堆（创建后调用一次）。
    fn rebuild_heap(&mut self) {
        self.heap.clear();
        for i in 0..self.tracks.len() {
            self.push_next_track(i);
        }
    }

    /// 将 `ti` 轨的当前事件推入归并堆（耗尽时不入堆）。
    fn push_next_track(&mut self, ti: usize) {
        let tick = self.tracks[ti].next_tick();
        if tick != u64::MAX {
            self.heap.push(Reverse((tick, ti)));
        }
    }
}
