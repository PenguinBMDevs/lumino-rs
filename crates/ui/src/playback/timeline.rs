//! 时间线管理模块
//!
//! 负责根据速度变化将tick转换为实际时间

use super::tempo::{TempoChange, bpm_from_tempo};

/// 时间线管理器
///
/// 负责根据速度变化将tick转换为实际时间
#[derive(Debug, Clone)]
pub struct Timeline {
    /// MIDI时间分辨率（ticks per quarter note）
    pub division: u16,
    /// 速度变化列表（按tick排序）
    tempo_changes: Vec<TempoChange>,
}

impl Timeline {
    /// 创建新的时间线
    pub fn new(division: u16) -> Self {
        Self {
            division,
            tempo_changes: vec![TempoChange::from_bpm(0.0, 120.0)], // 默认120 BPM
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
    }

    /// 添加单个速度变化
    pub fn add_tempo_change(&mut self, change: TempoChange) {
        self.tempo_changes.push(change);
        // 使用 total_cmp 进行安全的浮点数比较
        self.tempo_changes.sort_by(|a, b| a.tick.total_cmp(&b.tick));
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
        self.tempo_changes
            .iter()
            .rev()
            .find(|tc| tc.tick <= tick)
            .map(|tc| tc.tempo)
            .unwrap_or(DEFAULT_TEMPO)
    }

    /// 将tick转换为微秒
    pub fn tick_to_microseconds(&self, tick: f32) -> u64 {
        let mut current_tick = 0.0;
        let mut total_microseconds = 0u64;

        for (i, tempo_change) in self.tempo_changes.iter().enumerate() {
            let next_change_tick = self
                .tempo_changes
                .get(i + 1)
                .map(|tc| tc.tick)
                .unwrap_or(f32::MAX);

            if tick <= tempo_change.tick {
                // 目标在此速度段之前
                break;
            }

            let segment_end = tick.min(next_change_tick);
            let delta_ticks = segment_end - tempo_change.tick.max(current_tick);

            if delta_ticks > 0.0 {
                // 微秒 = (tick数 / division) * tempo
                let microseconds =
                    (delta_ticks as f64 / self.division as f64) * tempo_change.tempo as f64;
                total_microseconds += microseconds.round() as u64;
                current_tick = segment_end;
            }

            if tick <= segment_end {
                break;
            }
        }

        total_microseconds
    }

    /// 将微秒转换为tick（用于从时间反查位置）
    pub fn microseconds_to_tick(&self, target_microseconds: u64) -> f32 {
        let mut accumulated_microseconds = 0u64;
        let mut current_tick = 0.0;

        for (i, tempo_change) in self.tempo_changes.iter().enumerate() {
            let next_change_tick = self.tempo_changes.get(i + 1).map(|tc| tc.tick);

            if let Some(next_tick) = next_change_tick {
                // 计算这个速度段最多能消耗多少时间
                let segment_ticks = next_tick - tempo_change.tick;
                let segment_microseconds =
                    (segment_ticks as f64 / self.division as f64) * tempo_change.tempo as f64;
                let segment_microseconds_u64 = segment_microseconds.round() as u64;

                if accumulated_microseconds + segment_microseconds_u64 >= target_microseconds {
                    // 目标在此速度段内
                    let remaining = target_microseconds.saturating_sub(accumulated_microseconds);
                    let ticks_in_segment =
                        (remaining as f64 * self.division as f64) / tempo_change.tempo as f64;
                    return tempo_change.tick + ticks_in_segment as f32;
                }

                accumulated_microseconds += segment_microseconds_u64;
                current_tick = next_tick;
            } else {
                // 最后一个速度段，延伸到无限远
                let remaining = target_microseconds.saturating_sub(accumulated_microseconds);
                let ticks_in_segment =
                    (remaining as f64 * self.division as f64) / tempo_change.tempo as f64;
                return tempo_change.tick + ticks_in_segment as f32;
            }
        }

        current_tick
    }
}
