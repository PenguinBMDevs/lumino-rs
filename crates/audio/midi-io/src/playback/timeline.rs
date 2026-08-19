//! 时间线管理模块
//!
//! 负责根据速度变化将tick转换为实际时间。
//!
//! # 性能策略（前缀和 + 二分查找）
//!
//! 早期实现对 `tick_to_microseconds` / `microseconds_to_tick` 都用 O(N) 线性扫描
//! `tempo_changes`。由于 `microseconds_to_tick` 在 `Playback::current_tick` 中被每帧调用，
//! 且持有 `parking_lot::Mutex` 锁，扫描段数随 `target_microseconds`（即播放时长）增长，
//! 导致锁持有时间随播放时长线性上升——播放越久每帧越慢。
//!
//! 现在维护 `cumulative_micros` 前缀和数组：`cumulative_micros[i]` 是从播放起点到
//! `tempo_changes[i].tick` 的累积微秒数。查询时用 `partition_point` 二分定位段索引，
//! 复杂度降为 O(log N)，与播放时长无关。

use crate::playback::tempo::{TempoChange, bpm_from_tempo};

/// 时间线管理器
///
/// 负责根据速度变化将tick转换为实际时间
#[derive(Debug, Clone)]
pub struct Timeline {
    /// MIDI时间分辨率（ticks per quarter note）
    pub division: u16,
    /// 速度变化列表（按tick排序）
    tempo_changes: Vec<TempoChange>,
    /// 每个速度段起点的累积微秒数前缀和。
    ///
    /// `cumulative_micros[i]` 等于从播放起点到 `tempo_changes[i].tick` 的微秒数，
    /// 用 `tempo_changes[i-1].tempo` 计算段 `i-1 → i` 的时长。最后一个段延伸到无穷远。
    /// 长度等于 `tempo_changes.len()`，`cumulative_micros[0]` 恒为 0。
    cumulative_micros: Vec<u64>,
}

impl Timeline {
    /// 创建新的时间线
    pub fn new(division: u16) -> Self {
        Self {
            division,
            tempo_changes: vec![TempoChange::from_bpm(0.0, 120.0)], // 默认120 BPM
            cumulative_micros: vec![0],
        }
    }

    /// 设置速度变化列表
    pub fn set_tempo_changes(&mut self, mut changes: Vec<TempoChange>) {
        if changes.is_empty() {
            changes.push(TempoChange::from_bpm(0.0, 120.0));
        }
        // 使用 total_cmp 进行安全的浮点数比较
        changes.sort_by(|a, b| a.tick.total_cmp(&b.tick));
        self.tempo_changes = changes;
        self.rebuild_cumulative();
    }

    /// 添加单个速度变化
    pub fn add_tempo_change(&mut self, change: TempoChange) {
        self.tempo_changes.push(change);
        // 使用 total_cmp 进行安全的浮点数比较
        self.tempo_changes.sort_by(|a, b| a.tick.total_cmp(&b.tick));
        self.rebuild_cumulative();
    }

    /// 重新计算 `cumulative_micros` 前缀和。
    ///
    /// 在 `set_tempo_changes` / `add_tempo_change` 后调用，保证前缀和与 `tempo_changes` 同步。
    fn rebuild_cumulative(&mut self) {
        let n = self.tempo_changes.len();
        self.cumulative_micros = Vec::with_capacity(n);
        let mut acc: u64 = 0;
        let mut last_tick: f32 = 0.0;
        for (i, tc) in self.tempo_changes.iter().enumerate() {
            if i > 0 {
                // 段 i-1 → i 用 tempo_changes[i-1].tempo 计算时长
                let prev_tempo = self.tempo_changes[i - 1].tempo;
                let delta_ticks = (tc.tick - last_tick).max(0.0);
                let micros = (delta_ticks as f64 / self.division as f64) * prev_tempo as f64;
                acc = acc.saturating_add(micros.round() as u64);
            }
            self.cumulative_micros.push(acc);
            last_tick = tc.tick;
        }
    }

    /// 获取当前BPM（在指定tick处）
    pub fn get_bpm_at(&self, tick: f32) -> f64 {
        let tempo = self.get_tempo_at(tick);
        bpm_from_tempo(tempo)
    }

    /// 获取当前tempo（在指定tick处）
    fn get_tempo_at(&self, tick: f32) -> u32 {
        // 默认120 BPM = 500000 微秒/拍
        const DEFAULT_TEMPO: u32 = 500_000;
        // 二分查找：最大的 i 使得 tempo_changes[i].tick <= tick
        if self.tempo_changes.is_empty() {
            return DEFAULT_TEMPO;
        }
        let idx = self
            .tempo_changes
            .partition_point(|tc| tc.tick <= tick)
            .saturating_sub(1);
        self.tempo_changes
            .get(idx)
            .map(|tc| tc.tempo)
            .unwrap_or(DEFAULT_TEMPO)
    }

    /// 将tick转换为微秒
    ///
    /// 二分定位 tick 落在哪个速度段，再用线性公式计算段内微秒数，O(log N)。
    pub fn tick_to_microseconds(&self, tick: f32) -> u64 {
        if self.tempo_changes.is_empty() {
            return 0;
        }
        // 找最大的 i 使得 tempo_changes[i].tick <= tick
        let seg = self
            .tempo_changes
            .partition_point(|tc| tc.tick <= tick)
            .saturating_sub(1)
            .min(self.tempo_changes.len() - 1);
        let seg_start_micros = self.cumulative_micros[seg];
        let seg_start_tick = self.tempo_changes[seg].tick;
        let tempo = self.tempo_changes[seg].tempo;
        let delta_ticks = (tick - seg_start_tick).max(0.0);
        let micros = (delta_ticks as f64 / self.division as f64) * tempo as f64;
        seg_start_micros.saturating_add(micros.round() as u64)
    }

    /// 将微秒转换为tick（用于从时间反查位置）
    ///
    /// 二分定位 target_microseconds 落在哪个速度段，再用线性公式计算段内 tick，O(log N)。
    /// 该方法在 `Playback::current_tick` 中被每帧调用，且处于锁内——O(log N) 保证
    /// 锁持有时间与播放时长无关，消除"播放越久每帧越慢"的线性上升问题。
    pub fn microseconds_to_tick(&self, target_microseconds: u64) -> f32 {
        if self.tempo_changes.is_empty() {
            return 0.0;
        }
        // 找最大的 i 使得 cumulative_micros[i] <= target
        // partition_point 返回第一个 > target 的位置，所以段索引 = idx - 1
        let idx = self
            .cumulative_micros
            .partition_point(|&m| m <= target_microseconds);
        let seg = idx.saturating_sub(1).min(self.tempo_changes.len() - 1);
        let seg_start_micros = self.cumulative_micros[seg];
        let seg_start_tick = self.tempo_changes[seg].tick;
        let tempo = self.tempo_changes[seg].tempo;
        let remaining = target_microseconds.saturating_sub(seg_start_micros);
        let ticks_in_segment = (remaining as f64 * self.division as f64) / tempo as f64;
        seg_start_tick + ticks_in_segment as f32
    }
}
