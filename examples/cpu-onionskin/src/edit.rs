//! 编辑层：选择、命中、拖动、插入/删除/复制粘贴、撤销重做。
//!
//! 编辑直接改 `Arc<MidiDocument>`（`Arc::make_mut`，块级 COW），渲染层始终读
//! 同一个 `Arc`，不复制第二份音符数据。派生索引/密度金字塔按增量更新；
//! 撤销/重做恢复 Arc 快照并由 App 重建密度。

mod drag;
mod selection;

#[cfg(test)]
mod tests;

use std::{collections::BTreeSet, sync::Arc};

use lumino_midi_loader::{MidiDocument, NoteEvent};

use crate::{model::TrackIndex, render::density::DensityPyramid};

/// 选择集上限：避免一次框选 800 万音符把 BTreeSet 拖成秒级。
pub const MAX_SELECTION: usize = 200_000;
const HISTORY_LIMIT: usize = 24;
/// 单笔历史可回放的按值变更上限；超过则撤销时整建密度。
const MAX_HISTORY_CHANGES: usize = 64;

/// 当前工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Insert,
}

/// 剪贴板（内部格式，保持相对位置）。
#[derive(Debug, Default)]
pub struct Clipboard {
    notes: Vec<NoteEvent>,
    anchor_tick: u32,
    anchor_key: u8,
}

/// 历史按值变更：`old`/`new` 其一为 `None` 表达增删。
#[derive(Debug, Clone, Copy)]
struct NoteChange {
    old: Option<NoteEvent>,
    new: Option<NoteEvent>,
}

struct HistoryEntry {
    document: Arc<MidiDocument>,
    index: Arc<TrackIndex>,
    changes: Vec<NoteChange>,
    full: bool,
}

/// 拖动状态。
#[derive(Debug)]
pub enum Drag {
    Box {
        start_tick: u32,
        start_key: u8,
        current_tick: u32,
        current_key: u8,
    },
    Move {
        start_tick: u32,
        start_key: u8,
        delta_tick: i32,
        delta_key: i32,
        notes: Vec<NoteEvent>,
    },
}

/// 编辑状态。
pub struct Editor {
    pub tool: Tool,
    pub selection: BTreeSet<usize>,
    pub drag: Option<Drag>,
    pub clipboard: Clipboard,
    pub status: String,
    pub density_dirty: bool,
    history: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    snap_ticks: u32,
}

impl Editor {
    #[must_use]
    pub fn new(ppq: u32) -> Self {
        Self {
            tool: Tool::Select,
            selection: BTreeSet::new(),
            drag: None,
            clipboard: Clipboard::default(),
            status: "select | 左键框选/拖动，Tab 插入，Del 删除，Ctrl+C/V/Z/Y".to_string(),
            density_dirty: false,
            history: Vec::new(),
            redo: Vec::new(),
            snap_ticks: (ppq / 16).max(1),
        }
    }

    #[must_use]
    pub fn snap_ticks(&self) -> u32 {
        self.snap_ticks
    }

    #[must_use]
    pub fn tool_name(&self) -> &'static str {
        match self.tool {
            Tool::Select => "select",
            Tool::Insert => "insert",
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    #[must_use]
    pub fn selection_count(&self) -> usize {
        self.selection.len()
    }

    pub fn snapshot(&mut self, document: &Arc<MidiDocument>, index: &Arc<TrackIndex>) {
        self.history.push(HistoryEntry {
            document: Arc::clone(document),
            index: Arc::clone(index),
            changes: Vec::new(),
            full: false,
        });
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.redo.clear();
    }

    /// 把本笔操作的按值变更挂到刚推入的历史条目上。
    fn record_change(&mut self, old: Option<NoteEvent>, new: Option<NoteEvent>) {
        let Some(entry) = self.history.last_mut() else {
            return;
        };
        if entry.full {
            return;
        }
        if entry.changes.len() >= MAX_HISTORY_CHANGES {
            entry.changes.clear();
            entry.full = true;
            return;
        }
        entry.changes.push(NoteChange { old, new });
    }

    /// 把历史变更回放到密度（逆序 = 撤销）。
    fn apply_history_density(
        &mut self,
        density: &mut DensityPyramid,
        changes: &[NoteChange],
        inverse: bool,
    ) {
        let mut valid = true;
        for change in changes {
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

    /// 在 `(tick,key)` 插入一个 snap 长度音符。
    pub fn insert_at(
        &mut self,
        document: &mut Arc<MidiDocument>,
        index: &mut Arc<TrackIndex>,
        density: &mut DensityPyramid,
        track: usize,
        tick: u32,
        key: u8,
    ) {
        self.snapshot(document, index);
        let note = NoteEvent::new(tick, tick.saturating_add(self.snap_ticks), key, 100, 0);
        if !Arc::make_mut(document).insert_note(track, note) {
            return;
        }
        if !density.apply_note_range(note.start_tick, note.end_tick, note.key, 1) {
            self.density_dirty = true;
        }
        self.record_change(None, Some(note));
        *index = Arc::new(index.with_changes(track, &[], &[note]));
        self.selection.clear();
        if let Some(position) = document.track_notes(track).position_of(&note) {
            self.selection.insert(position);
        }
        self.status = format!("插入 1 个音符 @ tick {tick}");
    }

    /// 删除选中音符。
    pub fn delete_selected(
        &mut self,
        document: &mut Arc<MidiDocument>,
        index: &mut Arc<TrackIndex>,
        density: &mut DensityPyramid,
        track: usize,
    ) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let removed: Vec<NoteEvent> = self
            .selection
            .iter()
            .filter_map(|&position| document.track_notes(track).get(position).copied())
            .collect();
        if removed.is_empty() {
            return false;
        }
        self.snapshot(document, index);
        for position in self.selection.iter().rev() {
            Arc::make_mut(document).remove_note(track, *position);
        }
        for note in &removed {
            if !density.apply_note_range(note.start_tick, note.end_tick, note.key, -1) {
                self.density_dirty = true;
            }
            self.record_change(Some(*note), None);
        }
        *index = Arc::new(index.with_changes(track, &removed, &[]));
        self.selection.clear();
        self.status = format!("删除 {} 个音符", removed.len());
        true
    }

    pub fn copy_selected(&mut self, document: &MidiDocument, track: usize) {
        let notes = document.track_notes(track);
        let values: Vec<NoteEvent> = self
            .selection
            .iter()
            .filter_map(|&position| notes.get(position).copied())
            .collect();
        if values.is_empty() {
            return;
        }
        let anchor_tick = values.iter().map(|note| note.start_tick).min().unwrap_or(0);
        let anchor_key = values.iter().map(|note| note.key).min().unwrap_or(0);
        self.status = format!("已复制 {} 个音符", values.len());
        self.clipboard = Clipboard {
            notes: values,
            anchor_tick,
            anchor_key,
        };
    }

    pub fn paste_at(
        &mut self,
        document: &mut Arc<MidiDocument>,
        index: &mut Arc<TrackIndex>,
        density: &mut DensityPyramid,
        track: usize,
        tick: u32,
        key: u8,
    ) {
        if self.clipboard.notes.is_empty() {
            return;
        }
        self.snapshot(document, index);
        let offset_tick = i64::from(tick) - i64::from(self.clipboard.anchor_tick);
        let offset_key = i32::from(key) - i32::from(self.clipboard.anchor_key);
        let mut added = Vec::with_capacity(self.clipboard.notes.len());
        for note in &self.clipboard.notes {
            let start = (i64::from(note.start_tick) + offset_tick).max(0) as u32;
            let key = (i16::from(note.key) + offset_key as i16).clamp(0, 127) as u8;
            added.push(NoteEvent {
                start_tick: start,
                end_tick: start.saturating_add(note.length()),
                key,
                ..*note
            });
        }
        let inserted = Arc::make_mut(document).batch_insert_notes(track, added.to_vec());
        if inserted == 0 {
            return;
        }
        for note in &added {
            if !density.apply_note_range(note.start_tick, note.end_tick, note.key, 1) {
                self.density_dirty = true;
            }
            self.record_change(None, Some(*note));
        }
        *index = Arc::new(index.with_changes(track, &[], &added));
        self.selection.clear();
        let notes = document.track_notes(track);
        for note in &added {
            if let Some(position) = notes.position_of(note) {
                self.selection.insert(position);
            }
        }
        self.status = format!("粘贴 {inserted} 个音符");
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
            full: entry.full,
        });
        *document = entry.document;
        *index = entry.index;
        self.selection.clear();
        if entry.full {
            self.density_dirty = true;
        } else {
            self.apply_history_density(density, &entry.changes, true);
        }
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
            full: entry.full,
        });
        *document = entry.document;
        *index = entry.index;
        self.selection.clear();
        if entry.full {
            self.density_dirty = true;
        } else {
            self.apply_history_density(density, &entry.changes, false);
        }
        self.status = "重做".to_string();
        true
    }
}
