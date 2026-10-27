//! 选择：命中测试、框选、全选、点击多选。

use lumino_midi_loader::{MidiDocument, NoteEvent};

use super::{Editor, MAX_SELECTION};
use crate::model::{LOOKBACK_TICKS, TrackIndex};

impl Editor {
    /// 命中测试：返回当前轨全局索引与音符值。
    #[must_use]
    pub fn hit_test(
        document: &MidiDocument,
        index: &TrackIndex,
        track: usize,
        tick: u32,
        key: u8,
    ) -> Option<(usize, NoteEvent)> {
        let notes = document.track_notes(track);
        let lookback = index.lookback(track);
        let (lo, hi) = notes.window_range(tick, tick.saturating_add(1), lookback);
        for (position, note) in notes.iter_window(lo, hi) {
            if note.length() > LOOKBACK_TICKS {
                continue;
            }
            if note.key == key && note.start_tick <= tick && note.end_tick >= tick {
                return Some((position, *note));
            }
        }
        for note in index.long_notes(track) {
            if note.key == key
                && note.start_tick <= tick
                && note.end_tick >= tick
                && let Some(position) = notes.position_of(note)
            {
                return Some((position, *note));
            }
        }
        None
    }

    /// 框选当前轨与 tick/key 区间相交的音符。
    #[allow(clippy::too_many_arguments)]
    pub fn box_select(
        &mut self,
        document: &MidiDocument,
        index: &TrackIndex,
        track: usize,
        tick_a: u32,
        tick_b: u32,
        key_a: u8,
        key_b: u8,
        additive: bool,
    ) {
        if !additive {
            self.selection.clear();
        }
        let (tick_lo, tick_hi) = (tick_a.min(tick_b), tick_a.max(tick_b));
        let (key_lo, key_hi) = (key_a.min(key_b), key_a.max(key_b));
        let notes = document.track_notes(track);
        let lookback = index.lookback(track);
        let (lo, hi) = notes.window_range(tick_lo, tick_hi.saturating_add(1), lookback);
        let mut capped = false;
        for (position, note) in notes.iter_window(lo, hi) {
            if note.length() > LOOKBACK_TICKS {
                continue;
            }
            if note.key >= key_lo
                && note.key <= key_hi
                && note.start_tick <= tick_hi
                && note.end_tick >= tick_lo
            {
                if self.selection.len() >= MAX_SELECTION {
                    capped = true;
                    break;
                }
                self.selection.insert(position);
            }
        }
        if !capped {
            for note in index.long_notes(track) {
                if note.key >= key_lo
                    && note.key <= key_hi
                    && note.start_tick <= tick_hi
                    && note.end_tick >= tick_lo
                {
                    if self.selection.len() >= MAX_SELECTION {
                        capped = true;
                        break;
                    }
                    if let Some(position) = notes.position_of(note) {
                        self.selection.insert(position);
                    }
                }
            }
        }
        self.status = if capped {
            format!("框选达到上限 {MAX_SELECTION}，请缩小范围")
        } else {
            format!("已选 {} 个音符", self.selection.len())
        };
    }

    pub fn select_all(&mut self, document: &MidiDocument, track: usize) {
        self.selection.clear();
        let count = document.track_note_count(track as u16) as usize;
        for index in 0..count.min(MAX_SELECTION) {
            self.selection.insert(index);
        }
        self.status = format!("全选 {} / {}", self.selection.len(), count);
    }

    /// 左键点击选择（ctrl 切换 / shift 追加 / 普通单选）。
    pub fn click_select(&mut self, position: usize, ctrl: bool, shift: bool) {
        if ctrl && self.selection.contains(&position) {
            self.selection.remove(&position);
        } else if ctrl || shift {
            self.selection.insert(position);
        } else {
            self.selection.clear();
            self.selection.insert(position);
        }
        self.status = format!("已选 {} 个音符", self.selection.len());
    }
}
