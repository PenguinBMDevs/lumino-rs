//! 计数器模式统计状态（每帧推进）
//!
//! 统计口径参考 Zenith-MIDI NoteCountRender 与 fmr NoteCounter：
//! - `note_count`：已开始的音符总数（start_tick <= 当前 tick，模仿 Zenith 的 meta 标记法，
//!   但利用 lumino 每轨按 start_tick 有序的特性用二分游标推进，O(Δ) 而非 O(N)）
//! - `polyphony`：当前正在发声的音符（start <= tick < end，模仿 fmr 的增量事件推进）
//! - `nps`：最近 1 秒（fps 帧）新开始音符数（模仿 fmr 的 frameQueue 滑动窗口）
//!
//! tick 单调递增推进；若出现回退（防御），自动重置为干净状态。

use std::collections::{BinaryHeap, VecDeque};

use lumino_event::window::video::{
    CounterAlignment, CounterFont, CounterSeparator, NoteCounterConfig,
};
use lumino_midi_loader::MidiDocument;

/// 计数器渲染配置（后台渲染线程使用，由事件层配置转换而来）。
#[derive(Debug, Clone)]
pub struct CounterRenderConfig {
    pub text: String,
    pub alignment: CounterAlignment,
    pub font_size: u32,
    pub font: CounterFont,
    pub separator: CounterSeparator,
    pub padding_zeroes: bool,
    pub bpm_int_pad: u32,
    pub bpm_dec_pad: u32,
    pub note_count_pad: u32,
    pub polyphony_pad: u32,
    pub nps_pad: u32,
    pub ticks_pad: u32,
    pub bars_pad: u32,
    pub frames_pad: u32,
    pub save_csv: bool,
    pub csv_output: std::path::PathBuf,
    pub csv_format: String,
}

impl From<&NoteCounterConfig> for CounterRenderConfig {
    fn from(cfg: &NoteCounterConfig) -> Self {
        Self {
            text: cfg.text.clone(),
            alignment: cfg.alignment,
            font_size: cfg.font_size.max(1),
            font: cfg.font.clone(),
            separator: cfg.separator,
            padding_zeroes: cfg.padding_zeroes,
            bpm_int_pad: cfg.bpm_int_pad.max(1),
            bpm_dec_pad: cfg.bpm_dec_pad,
            note_count_pad: cfg.note_count_pad.max(1),
            polyphony_pad: cfg.polyphony_pad.max(1),
            nps_pad: cfg.nps_pad.max(1),
            ticks_pad: cfg.ticks_pad.max(1),
            bars_pad: cfg.bars_pad.max(1),
            frames_pad: cfg.frames_pad.max(1),
            save_csv: cfg.save_csv,
            csv_output: std::path::PathBuf::from(&cfg.csv_output),
            csv_format: cfg.csv_format.clone(),
        }
    }
}

/// 计数器统计状态。
#[derive(Debug, Default)]
pub struct CounterStats {
    /// 每轨已计入 `note_count` 的音符游标（第一个未计数的索引）
    track_cursors: Vec<usize>,
    /// 活动音符（最小堆，按 end_tick 升序）：`(end_tick, track_idx)`
    active: BinaryHeap<std::cmp::Reverse<(u32, u16)>>,
    /// 最近 fps 帧每帧新开始音符数（滑动窗口，用于 NPS）
    note_deltas: VecDeque<u64>,
    /// 已开始的音符总数
    pub note_count: u64,
    /// 当前复音数（正在发声的音符）
    pub polyphony: u64,
    /// 复音数峰值
    pub max_polyphony: u64,
    /// 最近 1 秒新开始音符数
    pub nps: u64,
    /// NPS 峰值
    pub max_nps: u64,
    /// 已渲染帧数
    pub frames: u64,
    /// 上一帧 tick（检测回退）
    last_tick: u32,
}

impl CounterStats {
    /// 重置为初始状态（轨道游标按文档轨道数初始化）。
    pub fn reset(&mut self, document: &MidiDocument) {
        self.track_cursors = vec![0; document.notes.len()];
        self.active.clear();
        self.note_deltas.clear();
        self.note_count = 0;
        self.polyphony = 0;
        self.max_polyphony = 0;
        self.nps = 0;
        self.max_nps = 0;
        self.frames = 0;
        self.last_tick = 0;
    }

    /// 每帧推进统计到指定 tick（tick 必须单调不减）。
    ///
    /// 过程：
    /// 1. 每轨二分推进游标，将新开始的音符计入 `note_count` 并加入活动集合；
    /// 2. 从活动集合弹出已结束（end <= tick）的音符，`polyphony = active.len()`；
    /// 3. 将本帧新开始数压入 NPS 滑动窗口（保留最近 fps 帧）。
    pub fn advance(&mut self, document: &MidiDocument, tick: u32, fps: u32) {
        if tick < self.last_tick {
            // 防御：tick 回退（正常渲染不会发生），重置为干净状态
            self.reset(document);
        }
        let fps_usize = (fps.max(1)) as usize;
        let mut new_notes: u64 = 0;

        // 1. 推进每轨游标：start_tick <= tick 的音符计入 note_count
        for (track_idx, track_notes) in document.notes.iter().enumerate() {
            let cursor = match self.track_cursors.get_mut(track_idx) {
                Some(c) => *c,
                None => {
                    self.track_cursors.push(0);
                    0
                }
            };
            let upper = track_notes.partition_point(tick.saturating_add(1));
            if upper > cursor {
                for i in cursor..upper {
                    if let Some(n) = track_notes.get(i) {
                        self.active
                            .push(std::cmp::Reverse((n.end_tick, track_idx as u16)));
                        new_notes += 1;
                    }
                }
                self.track_cursors[track_idx] = upper;
            }
        }

        // 2. 弹出已结束的音符（end <= tick）
        while let Some(std::cmp::Reverse((end_tick, _))) = self.active.peek() {
            if *end_tick <= tick {
                self.active.pop();
            } else {
                break;
            }
        }
        self.polyphony = self.active.len() as u64;
        self.max_polyphony = self.max_polyphony.max(self.polyphony);

        // 3. 累计已开始音符总数 + NPS 滑动窗口
        self.note_count += new_notes;
        self.note_deltas.push_back(new_notes);
        while self.note_deltas.len() > fps_usize {
            self.note_deltas.pop_front();
        }
        self.nps = self.note_deltas.iter().sum();
        self.max_nps = self.max_nps.max(self.nps);

        self.frames += 1;
        self.last_tick = tick;
    }
}

/// 当前 tick 处的 BPM（tempo_changes 二分；空列表回退 120）。
pub(crate) fn current_bpm(tempo_changes: &[(u32, f32)], tick: u32) -> f64 {
    match tempo_changes.binary_search_by_key(&tick, |&(t, _)| t) {
        Ok(i) => tempo_changes[i].1 as f64,
        Err(0) => tempo_changes
            .first()
            .map(|&(_, b)| b as f64)
            .unwrap_or(120.0),
        Err(i) => tempo_changes[i - 1].1 as f64,
    }
}

/// 当前 tick 处的拍号（time_signatures 二分；空列表回退 4/4）。
/// 返回 `(分子, 分母)`。
pub(super) fn current_time_signature(time_signatures: &[(u32, u8, u8)], tick: u32) -> (u8, u8) {
    match time_signatures.binary_search_by_key(&tick, |&(t, _, _)| t) {
        Ok(i) => (time_signatures[i].1, time_signatures[i].2),
        Err(0) => time_signatures
            .first()
            .map(|&(_, n, d)| (n, d))
            .unwrap_or((4, 4)),
        Err(i) => (time_signatures[i - 1].1, time_signatures[i - 1].2),
    }
}

/// 将 tick 转换为秒（tempo 分段积分，与 streaming.rs 的 ticks_to_seconds 等价）。
pub(super) fn ticks_to_seconds(tick: u32, tempo_changes: &[(u32, f32)], ppq: u32) -> f64 {
    if ppq == 0 {
        return tick as f64;
    }
    let mut total_secs = 0.0_f64;
    let mut prev_tick = 0u32;
    let mut prev_bpm = 120.0f64;
    for &(t, bpm) in tempo_changes {
        let seg_ticks = t.saturating_sub(prev_tick) as f64;
        let seg_secs = seg_ticks * 60.0 / (prev_bpm * ppq as f64);
        total_secs += seg_secs;
        if tick <= t {
            let within = tick.saturating_sub(prev_tick) as f64;
            return total_secs - seg_secs + within * 60.0 / (prev_bpm * ppq as f64);
        }
        prev_tick = t;
        prev_bpm = bpm as f64;
    }
    let remaining = tick.saturating_sub(prev_tick) as f64;
    total_secs + remaining * 60.0 / (prev_bpm * ppq as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_loader::{NoteEvent, TrackManager};

    fn make_doc(tracks: &[Vec<(u32, u32, u8)>]) -> MidiDocument {
        MidiDocument {
            notes: tracks
                .iter()
                .map(|v| {
                    let mut list: Vec<NoteEvent> = v
                        .iter()
                        .map(|&(s, e, k)| NoteEvent::new(s, e, k, 100, 0))
                        .collect();
                    list.sort_unstable_by_key(|n| n.start_tick);
                    lumino_midi_loader::ChunkedList::from_sorted(list)
                })
                .collect(),
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![(0, 0, false)],
            control_events: lumino_midi_loader::ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![Some("T1".into())],
            total_ticks: 100_000,
            track_count: 1,
            tracks: TrackManager::new(1),
            division: 480,
            track_ports: vec![],
            track_max_end_ticks: vec![],
        }
    }

    /// 计数正确性：note_count / polyphony 与暴力全扫一致（含跨 tick 长音、叠音）
    #[test]
    fn test_advance_matches_full_scan() {
        let doc = make_doc(&[vec![
            (0, 480, 60),     // tick0 开始
            (0, 960, 62),     // 叠音
            (480, 1440, 64),  // tick480 开始
            (960, 1200, 66),  // 与 64 重叠
            (2000, 3000, 68), // 长音跨多帧
        ]]);

        let mut stats = CounterStats::default();
        stats.reset(&doc);
        let fps = 60u32;

        // 逐帧推进（每帧 480 tick 模拟 1 拍）
        for tick in [0u32, 480, 960, 1440, 1920, 2400, 3000, 3600] {
            stats.advance(&doc, tick, fps);
            // 暴力全扫
            let expected_count: u64 = doc
                .notes
                .iter()
                .map(|t| t.iter().filter(|n| n.start_tick <= tick).count() as u64)
                .sum();
            let expected_poly: u64 = doc
                .notes
                .iter()
                .flat_map(|t| t.iter())
                .filter(|n| n.start_tick <= tick && n.end_tick > tick)
                .count() as u64;
            assert_eq!(stats.note_count, expected_count, "tick={tick} note_count");
            assert_eq!(stats.polyphony, expected_poly, "tick={tick} polyphony");
        }
    }

    /// NPS：一帧推进 5 个音符，fps=60 时窗口内总和为 5
    #[test]
    fn test_nps_sliding_window() {
        // 100 个音符，每 48 tick 一个，1 秒（60fps × 480tick）内约 10 个
        let notes: Vec<(u32, u32, u8)> = (0..100).map(|i| (i * 48, i * 48 + 240, 60)).collect();
        let doc = make_doc(&[notes]);

        let mut stats = CounterStats::default();
        stats.reset(&doc);
        let fps = 60u32;

        // 推进 60 帧到 tick 28800；100 个音符（48 tick 间隔）在前 10 帧全部开始
        for frame in 0..60u32 {
            let tick = frame * 480;
            stats.advance(&doc, tick, fps);
        }
        // 全部 100 个音符已开始
        assert_eq!(stats.note_count, 100);
        // 窗口保留最近 60 帧 → 覆盖全部开始过程 → nps = 100
        assert_eq!(stats.nps, 100);
        assert_eq!(stats.frames, 60);

        // 窗口滑出：再推进 60 帧后，开始过程全部滑出窗口 → nps = 0
        for frame in 60..120u32 {
            let tick = frame * 480;
            stats.advance(&doc, tick, fps);
        }
        assert_eq!(stats.note_count, 100);
        assert_eq!(stats.nps, 0, "滑动窗口滑出后 nps 应为 0");
    }

    /// tick 回退防御：重置后统计正确
    #[test]
    fn test_advance_tick_regression_resets() {
        let doc = make_doc(&[vec![(0, 480, 60), (480, 960, 62)]]);
        let mut stats = CounterStats::default();
        stats.reset(&doc);
        stats.advance(&doc, 960, 60);
        assert_eq!(stats.note_count, 2);
        // 回退到 0 → 重置
        stats.advance(&doc, 0, 60);
        assert_eq!(
            stats.note_count, 1,
            "tick 回退应重置并重新计数 tick=0 的音符"
        );
        assert_eq!(stats.frames, 1);
    }

    /// 空文档不 panic
    #[test]
    fn test_advance_empty_doc() {
        let doc = make_doc(&[]);
        let mut stats = CounterStats::default();
        stats.reset(&doc);
        stats.advance(&doc, 0, 60);
        stats.advance(&doc, 480, 60);
        assert_eq!(stats.note_count, 0);
        assert_eq!(stats.polyphony, 0);
    }

    /// BPM 查询：区间内返回上一 tempo；边界正确
    #[test]
    fn test_current_bpm() {
        let tempos = vec![(0u32, 120.0f32), (480, 60.0), (960, 90.0)];
        assert_eq!(current_bpm(&tempos, 0), 120.0);
        assert_eq!(current_bpm(&tempos, 100), 120.0);
        assert_eq!(current_bpm(&tempos, 480), 60.0);
        assert_eq!(current_bpm(&tempos, 960), 90.0);
        assert_eq!(current_bpm(&tempos, 99999), 90.0);
        assert_eq!(current_bpm(&[], 0), 120.0, "空列表回退 120");
    }

    /// 拍号查询：区间内返回上一拍号；空列表回退 4/4
    #[test]
    fn test_current_time_signature() {
        let sigs = vec![(0u32, 4u8, 4u8), (480, 3, 4), (960, 6, 8)];
        assert_eq!(current_time_signature(&sigs, 0), (4, 4));
        assert_eq!(current_time_signature(&sigs, 240), (4, 4));
        assert_eq!(current_time_signature(&sigs, 480), (3, 4));
        assert_eq!(current_time_signature(&sigs, 1000), (6, 8));
        assert_eq!(current_time_signature(&[], 0), (4, 4));
    }

    /// tick→秒：120 BPM 下 1 拍（480 tick）= 0.5 秒
    #[test]
    fn test_ticks_to_seconds() {
        let tempos = vec![(0u32, 120.0f32), (960, 60.0)];
        assert!((ticks_to_seconds(480, &tempos, 480) - 0.5).abs() < 1e-9);
        assert!((ticks_to_seconds(960, &tempos, 480) - 1.0).abs() < 1e-9);
        // 60 BPM 段：960→1920 tick = 2 秒；总 1.0 + 2.0 = 3.0 秒
        assert!((ticks_to_seconds(1920, &tempos, 480) - 3.0).abs() < 1e-9);
        // 空 tempo 回退 120 BPM：480 tick = 0.5 秒
        assert!((ticks_to_seconds(480, &[], 480) - 0.5).abs() < 1e-9);
    }
}
