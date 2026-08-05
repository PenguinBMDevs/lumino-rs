//! 编辑器音符变换操作
//!
//! 从 EditorData 中提取的变换方法：翻转、移调、变速。
//! 这些操作不直接属于 EditorData 的核心职责，因此提取为 trait 扩展。

use std::collections::HashSet;

use crate::EditorData;
use lumino_note_core::midi_types::VelocityPoint;

/// 音符变换操作 trait
pub trait EditorTransform {
    /// 垂直翻转选中音符
    fn flip_vertical(&mut self, selected: &HashSet<usize>, max_key_index: f32) -> usize;

    /// 水平翻转选中音符
    fn flip_horizontal(&mut self, selected: &HashSet<usize>, axis_tick: f32) -> usize;

    /// 移调选中音符（或全部音符）
    fn transpose(&mut self, selected: &HashSet<usize>, semitones: i16) -> usize;

    /// 变速选中音符（或全部音符）
    fn apply_speed_change(&mut self, selected: &HashSet<usize>, speed_factor: f32) -> usize;

    /// 构建力度点
    fn build_velocity_points(&self) -> Vec<VelocityPoint>;
}

impl EditorTransform for EditorData {
    fn flip_vertical(&mut self, selected: &HashSet<usize>, max_key_index: f32) -> usize {
        let selected_indices: Vec<usize> = selected.iter().copied().collect();
        if selected_indices.is_empty() {
            return 0;
        }
        let mut min_key = u16::MAX;
        let mut max_key = u16::MIN;
        for &note_idx in &selected_indices {
            if let Some(note) = self.notes.get(note_idx) {
                min_key = min_key.min(note.key);
                max_key = max_key.max(note.key);
            }
        }
        if min_key > max_key {
            return 0;
        }
        let center = (min_key as f32 + max_key as f32) / 2.0;
        self.push_history();
        let mut modified = 0;
        for &note_idx in &selected_indices {
            if let Some(note) = self.notes.get_mut(note_idx) {
                let new_key = (2.0 * center - note.key as f32)
                    .round()
                    .clamp(0.0, max_key_index) as u16;
                if new_key != note.key {
                    note.key = new_key;
                    modified += 1;
                }
            }
        }
        if modified > 0 {
            // 增量对账：等长修改记录事件（内部整轨同步 + 清 dirty）
            self.record_update_ranges(&selected_indices);
        } else {
            self.history.discard_last();
        }
        modified
    }

    fn flip_horizontal(&mut self, selected: &HashSet<usize>, axis_tick: f32) -> usize {
        let selected_indices: Vec<usize> = selected.iter().copied().collect();
        if selected_indices.is_empty() {
            return 0;
        }
        self.push_history();
        let mut modified = 0;
        for &note_idx in &selected_indices {
            if let Some(note) = self.notes.get_mut(note_idx) {
                let new_tick = (2.0 * axis_tick - (note.tick + note.length)).max(0.0);
                if (new_tick - note.tick).abs() > f32::EPSILON {
                    note.tick = new_tick;
                    modified += 1;
                }
            }
        }
        if modified > 0 {
            self.record_update_ranges(&selected_indices);
        } else {
            self.history.discard_last();
        }
        modified
    }

    fn transpose(&mut self, selected: &HashSet<usize>, semitones: i16) -> usize {
        let notes_len = self.notes.len();
        let indices: Vec<usize> = if selected.is_empty() {
            (0..notes_len).collect()
        } else {
            selected.iter().copied().collect()
        };
        if indices.is_empty() {
            return 0;
        }
        self.push_history();
        let mut modified = 0;
        for &note_idx in &indices {
            if let Some(note) = self.notes.get_mut(note_idx) {
                let new_key = (note.key as i16 + semitones).clamp(0, 255) as u16;
                if new_key != note.key {
                    note.key = new_key;
                    modified += 1;
                }
            }
        }
        if modified > 0 {
            self.record_update_ranges(&indices);
        } else {
            self.history.discard_last();
        }
        modified
    }

    fn apply_speed_change(&mut self, selected: &HashSet<usize>, speed_factor: f32) -> usize {
        if self.notes.is_empty() {
            return 0;
        }
        let indices: Vec<usize> = if selected.is_empty() {
            (0..self.notes.len()).collect()
        } else {
            let mut v: Vec<usize> = selected.iter().copied().collect();
            v.sort();
            v
        };
        if indices.is_empty() {
            return 0;
        }
        let min_tick = indices
            .iter()
            .filter_map(|idx| self.notes.get(*idx).map(|note| note.tick))
            .fold(f32::INFINITY, f32::min);
        if min_tick.is_infinite() {
            return 0;
        }
        self.push_history();
        let mut modified = 0;
        const MIN_LEN: f32 = 1.0;
        for &note_idx in &indices {
            if let Some(note) = self.notes.get_mut(note_idx) {
                let new_tick = min_tick + (note.tick - min_tick) * speed_factor;
                let new_length = (note.length * speed_factor).max(MIN_LEN);
                if (new_tick - note.tick).abs() > f32::EPSILON
                    || (new_length - note.length).abs() > f32::EPSILON
                {
                    note.tick = new_tick;
                    note.length = new_length;
                    modified += 1;
                }
            }
        }
        if modified > 0 {
            self.record_update_ranges(&indices);
        } else {
            self.history.discard_last();
        }
        modified
    }

    fn build_velocity_points(&self) -> Vec<VelocityPoint> {
        let mut points: Vec<VelocityPoint> = self
            .notes
            .iter()
            .enumerate()
            .map(|(note_idx, note)| VelocityPoint {
                note_index: note_idx,
                tick: note.tick,
                velocity: note.velocity,
                length: note.length,
            })
            .collect();
        points.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.note_index.cmp(&b.note_index))
        });
        points
    }
}
