//! 循环回绕缓存（PREF-006 A2）
//!
//! `handle_loop_wrap` 旧实现每次回绕都对每个非当前轨执行
//! `0..partition_point(loop_start)` 全扫，找出仍在发声的悬挂音符；
//! 循环位于大文件尾部时，每次回绕都是 O(整个文件音符数)
//! （实测：Bad Apple 61MB ≈ 4.7ms/次、Night Voyager 153MB ≈ 11.8ms/次，
//! 后者已超过半块播放预算）。
//!
//! 观察：**循环配置（文档快照 + loop_start + 轨数）不变时**，
//! 「loop_start 处仍在发声」的悬挂 NoteOff 集合与各游标是常量。
//! 因此首次回绕构建一次缓存（O(N)），之后每次回绕只做 O(K) 克隆
//! （K = loop_start 处正在响的音符数），把稳态回绕从 O(N) 降为 O(K)。
//!
//! 失效点（任一变化都会 `loop_wrap_cache = None`）：
//! - `set_document`（编辑快照 / 换文档 / 换当前轨）；
//! - `set_loop_range` / `clear_loop_range`（loop_start 变化）；
//! - `set_midi_events`（额外事件游标依赖事件表）。

use std::collections::BinaryHeap;

use lumino_midi_loader::{ChunkedList, NoteEvent};

use super::core::{PendingNoteOff, PlaybackEngine};

/// 回绕缓存：循环配置不变时的游标与悬挂 NoteOff 常量集合
#[derive(Debug, Clone)]
pub(crate) struct LoopWrapCache {
    /// 缓存对应的 loop_start（同一循环配置下逐位相同）
    pub(crate) loop_start: f32,
    /// 每轨 `(note_cursor, pending_offs)`（PREF-006 A1：含当前轨）
    pub(crate) tracks: Vec<(usize, BinaryHeap<PendingNoteOff>)>,
    /// 控制事件游标
    pub(crate) control_cursor: usize,
    /// 额外 MIDI 事件游标
    pub(crate) midi_cursor: usize,
}

/// 扫描 `seek_tick` 处仍在发声的悬挂 NoteOff。
///
/// 音符按 `start_tick` 排序：`cursor = partition_point(seek_tick)` 之后，
/// 前缀中 `end_tick >= seek_tick` 的音符即「已开始但未结束」。
/// 与 `reset_cursors_to`（seek 路径）共用，保证 seek / 回绕口径一致。
pub(crate) fn scan_pending_offs(
    notes: &ChunkedList<NoteEvent>,
    seek_tick: u32,
) -> (usize, BinaryHeap<PendingNoteOff>) {
    let cursor = notes.partition_point(seek_tick);
    let mut pending_offs = BinaryHeap::new();
    for (note_idx, note) in notes.iter().enumerate().take(cursor) {
        if note.end_tick >= seek_tick {
            pending_offs.push(PendingNoteOff {
                end_tick: note.end_tick,
                note_index: note_idx,
            });
        }
    }
    (cursor, pending_offs)
}

impl PlaybackEngine {
    /// 确保回绕缓存与当前循环配置一致（命中直接返回；未命中/失效则重建）。
    pub(crate) fn ensure_loop_wrap_cache(&mut self, loop_start: f32) {
        if let Some(cache) = self.loop_wrap_cache.as_ref()
            && cache.loop_start == loop_start
            && cache.tracks.len() == self.track_states.len()
        {
            return;
        }
        let Some(doc) = self.document.as_ref() else {
            return;
        };
        let seek_tick = loop_start as u32;
        let mut tracks = Vec::with_capacity(self.track_states.len());
        for track_idx in 0..self.track_states.len() {
            // PREF-006 A1：当前轨也走流式模型，缓存覆盖全部轨。
            tracks.push(scan_pending_offs(doc.track_notes(track_idx), seek_tick));
        }
        self.loop_wrap_cache = Some(LoopWrapCache {
            loop_start,
            tracks,
            control_cursor: doc.control_events.partition_point(seek_tick),
            midi_cursor: self
                .midi_events
                .partition_point(|event| event.tick < loop_start),
        });
    }

    /// 把回绕缓存应用到各轨读取状态（稳态 O(K) 克隆）。
    pub(crate) fn apply_loop_wrap_cache(&mut self) {
        let Some(cache) = self.loop_wrap_cache.as_ref() else {
            return;
        };
        for (track_idx, state) in self.track_states.iter_mut().enumerate() {
            if let Some((cursor, offs)) = cache.tracks.get(track_idx) {
                state.note_cursor = *cursor;
                state.pending_offs = offs.clone();
            }
        }
        self.control_event_cursor = cache.control_cursor;
        self.midi_event_cursor = cache.midi_cursor;
    }
}
