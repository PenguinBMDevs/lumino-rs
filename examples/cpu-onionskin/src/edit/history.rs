//! 撤销/重做与密度回放（从 `edit.rs` 拆出，守住单文件 <400 行）。

use std::sync::Arc;

use lumino_midi_loader::{MidiDocument, NoteEvent};

use super::{
    Editor, HISTORY_LIMIT, HistoryEntry, MAX_HISTORY_CHANGES, NoteChange, note_change_range,
    push_history_range,
};
use crate::{model::TrackIndex, render::density::DensityPyramid};

impl Editor {
    pub fn snapshot(&mut self, document: &Arc<MidiDocument>, index: &Arc<TrackIndex>) {
        self.history.push(HistoryEntry {
            document: Arc::clone(document),
            index: Arc::clone(index),
            changes: Vec::new(),
            ranges: Vec::new(),
            range_mode: false,
            full: false,
        });
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.redo.clear();
    }

    /// 把本笔操作的按值变更挂到刚推入的历史条目上。
    pub(super) fn record_change(&mut self, old: Option<NoteEvent>, new: Option<NoteEvent>) {
        let Some(entry) = self.history.last_mut() else {
            return;
        };
        if entry.full {
            return;
        }
        if let Some((lo, hi)) = note_change_range(old, new) {
            push_history_range(&mut entry.ranges, lo, hi);
        }
        if entry.range_mode {
            return;
        }
        if entry.changes.len() < MAX_HISTORY_CHANGES {
            entry.changes.push(NoteChange { old, new });
        } else {
            // 逐音符变更超限：只保留区间，撤销时局部重建。
            entry.changes.clear();
            entry.range_mode = true;
        }
    }

    /// 把历史变更回放到密度（逆序 = 撤销）。
    fn apply_history_density(
        &mut self,
        density: &mut DensityPyramid,
        document: &MidiDocument,
        index: &TrackIndex,
        entry: &HistoryEntry,
        inverse: bool,
    ) {
        if entry.full {
            self.density_dirty = true;
            return;
        }
        if entry.range_mode && !entry.ranges.is_empty() {
            if !density.rebuild_dirty_ranges(document, index, &entry.ranges) {
                self.density_dirty = true;
            }
            return;
        }
        let mut valid = true;
        for change in &entry.changes {
            let (remove, add) = if inverse {
                (change.new, change.old)
            } else {
                (change.old, change.new)
            };
            if let Some(note) = remove {
                valid &= density.apply_note_range(note.start_tick, note.end_tick, note.key, -1);
            }
            if let Some(note) = add {
                valid &= density.apply_note_range(note.start_tick, note.end_tick, note.key, 1);
            }
        }
        if !valid {
            self.density_dirty = true;
        }
    }

    /// 撤销；成功返回 true。密度按历史变更增量回放，失败才整建。
    pub fn undo(
        &mut self,
        document: &mut Arc<MidiDocument>,
        index: &mut Arc<TrackIndex>,
        density: &mut DensityPyramid,
    ) -> bool {
        let Some(entry) = self.history.pop() else {
            return false;
        };
        self.redo.push(HistoryEntry {
            document: Arc::clone(document),
            index: Arc::clone(index),
            changes: entry.changes.clone(),
            ranges: entry.ranges.clone(),
            range_mode: entry.range_mode,
            full: entry.full,
        });
        self.apply_history_density(
            density,
            entry.document.as_ref(),
            entry.index.as_ref(),
            &entry,
            true,
        );
        *document = entry.document;
        *index = entry.index;
        self.selection.clear();
        self.status = "撤销".to_string();
        true
    }

    pub fn redo(
        &mut self,
        document: &mut Arc<MidiDocument>,
        index: &mut Arc<TrackIndex>,
        density: &mut DensityPyramid,
    ) -> bool {
        let Some(entry) = self.redo.pop() else {
            return false;
        };
        self.history.push(HistoryEntry {
            document: Arc::clone(document),
            index: Arc::clone(index),
            changes: entry.changes.clone(),
            ranges: entry.ranges.clone(),
            range_mode: entry.range_mode,
            full: entry.full,
        });
        self.apply_history_density(
            density,
            entry.document.as_ref(),
            entry.index.as_ref(),
            &entry,
            false,
        );
        *document = entry.document;
        *index = entry.index;
        self.selection.clear();
        self.status = "重做".to_string();
        true
    }
}
