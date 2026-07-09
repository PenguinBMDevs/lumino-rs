//! 编辑器音符变换操作
//!
//! 从 EditorData 中提取的变换方法：翻转、移调、变速。
//! 这些操作不直接属于 EditorData 的核心职责，因此提取为 trait 扩展。

use std::collections::HashSet;

use crate::editor_state::EditorData;
use crate::history::EditorSnapshot;
use crate::midi_types::VelocityPoint;

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
        let sel: Vec<usize> = selected.iter().copied().collect();
        if sel.is_empty() {
            return 0;
        }
        let mut min_key = u16::MAX;
        let mut max_key = u16::MIN;
        for &i in &sel {
            if let Some(n) = self.notes.get(i) {
                min_key = min_key.min(n.key);
                max_key = max_key.max(n.key);
            }
        }
        if min_key > max_key {
            return 0;
        }
        let center = (min_key as f32 + max_key as f32) / 2.0;
        self.push_history();
        let mut modified = 0;
        for &i in &sel {
            if let Some(n) = self.notes.get_mut(i) {
                let nk = (2.0 * center - n.key as f32)
                    .round()
                    .clamp(0.0, max_key_index) as u16;
                if nk != n.key {
                    n.key = nk;
                    modified += 1;
                }
            }
        }
        if modified > 0 {
            self.sync_track_notes();
        } else {
            self.history.undo(EditorSnapshot::new(
                self.notes.clone(),
                self.current_track,
                self.automation_lanes.clone(),
            ));
        }
        modified
    }

    fn flip_horizontal(&mut self, selected: &HashSet<usize>, axis_tick: f32) -> usize {
        let sel: Vec<usize> = selected.iter().copied().collect();
        if sel.is_empty() {
            return 0;
        }
        self.push_history();
        let mut modified = 0;
        for &i in &sel {
            if let Some(n) = self.notes.get_mut(i) {
                let nt = (2.0 * axis_tick - (n.tick + n.length)).max(0.0);
                if (nt - n.tick).abs() > f32::EPSILON {
                    n.tick = nt;
                    modified += 1;
                }
            }
        }
        if modified > 0 {
            self.sync_track_notes();
        } else {
            self.history.undo(EditorSnapshot::new(
                self.notes.clone(),
                self.current_track,
                self.automation_lanes.clone(),
            ));
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
        for &i in &indices {
            if let Some(n) = self.notes.get_mut(i) {
                let nk = (n.key as i16 + semitones).clamp(0, 255) as u16;
                if nk != n.key {
                    n.key = nk;
                    modified += 1;
                }
            }
        }
        if modified > 0 {
            self.sync_track_notes();
        } else {
            self.history.undo(EditorSnapshot::new(
                self.notes.clone(),
                self.current_track,
                self.automation_lanes.clone(),
            ));
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
            .filter_map(|i| self.notes.get(*i).map(|n| n.tick))
            .fold(f32::INFINITY, f32::min);
        if min_tick.is_infinite() {
            return 0;
        }
        self.push_history();
        let mut modified = 0;
        const MIN_LEN: f32 = 1.0;
        for &i in &indices {
            if let Some(n) = self.notes.get_mut(i) {
                let nt = min_tick + (n.tick - min_tick) * speed_factor;
                let nl = (n.length * speed_factor).max(MIN_LEN);
                if (nt - n.tick).abs() > f32::EPSILON || (nl - n.length).abs() > f32::EPSILON {
                    n.tick = nt;
                    n.length = nl;
                    modified += 1;
                }
            }
        }
        if modified > 0 {
            self.sync_track_notes();
        } else {
            self.history.undo(EditorSnapshot::new(
                self.notes.clone(),
                self.current_track,
                self.automation_lanes.clone(),
            ));
        }
        modified
    }

    fn build_velocity_points(&self) -> Vec<VelocityPoint> {
        let mut points: Vec<VelocityPoint> = self
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| VelocityPoint {
                note_index: i,
                tick: n.tick,
                velocity: n.velocity,
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
