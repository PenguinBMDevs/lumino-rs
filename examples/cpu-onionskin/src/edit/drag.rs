//! 拖动：移动预览、提交移动、框选提交。

use std::sync::Arc;

use lumino_midi_loader::{MidiDocument, NoteEvent};

use super::{Drag, Editor};
use crate::{model::TrackIndex, render::density::DensityPyramid};

impl Editor {
    /// 开始移动：收集选中音符值（后续拖动只画 ghost，不改文档）。
    pub fn begin_move(&mut self, document: &MidiDocument, track: usize, tick: u32, key: u8) {
        if self.selection.len() > 20_000 {
            self.status = "选择过大，ghost 预览已简化；释放后仍会移动".to_string();
        }
        let notes = document.track_notes(track);
        let values = self
            .selection
            .iter()
            .filter_map(|&position| notes.get(position).copied())
            .collect();
        self.drag = Some(Drag::Move {
            start_tick: tick,
            start_key: key,
            delta_tick: 0,
            delta_key: 0,
            notes: values,
        });
    }

    /// 更新拖动预览。
    pub fn update_drag(&mut self, tick: u32, key: u8) {
        let snap = self.snap_ticks;
        match &mut self.drag {
            Some(Drag::Box {
                current_tick,
                current_key,
                ..
            }) => {
                *current_tick = tick;
                *current_key = key;
            }
            Some(Drag::Move {
                start_tick,
                start_key,
                delta_tick,
                delta_key,
                ..
            }) => {
                let raw = i64::from(tick) - i64::from(*start_tick);
                let snapped = ((raw as f64) / f64::from(snap)).round() as i64 * i64::from(snap);
                *delta_tick = snapped.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
                *delta_key = i32::from(key) - i32::from(*start_key);
            }
            None => {}
        }
    }

    /// 提交移动；返回是否有文档变化。
    pub fn commit_move(
        &mut self,
        document: &mut Arc<MidiDocument>,
        index: &mut Arc<TrackIndex>,
        density: &mut DensityPyramid,
        track: usize,
    ) -> bool {
        let Some(Drag::Move {
            delta_tick,
            delta_key,
            notes,
            ..
        }) = self.drag.take()
        else {
            return false;
        };
        if delta_tick == 0 && delta_key == 0 {
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
        let mut added = Vec::with_capacity(notes.len());
        for note in &notes {
            let start = (i64::from(note.start_tick) + i64::from(delta_tick)).max(0) as u32;
            let key = (i16::from(note.key) + delta_key as i16).clamp(0, 127) as u8;
            let end = start.saturating_add(note.length());
            added.push(NoteEvent {
                start_tick: start,
                end_tick: end,
                key,
                ..*note
            });
        }
        for position in self.selection.iter().rev() {
            Arc::make_mut(document).remove_note(track, *position);
        }
        Arc::make_mut(document).batch_insert_notes(track, added.to_vec());
        for note in &removed {
            density.apply_note_range(note.start_tick, note.end_tick, note.key, -1);
        }
        for note in &added {
            density.apply_note_range(note.start_tick, note.end_tick, note.key, 1);
        }
        *index = Arc::new(index.with_changes(track, &removed, &added));
        self.selection.clear();
        self.status = format!("已移动 {} 个音符", added.len());
        true
    }

    /// 框选提交。
    pub fn commit_box(
        &mut self,
        document: &MidiDocument,
        index: &TrackIndex,
        track: usize,
        additive: bool,
    ) {
        let Some(Drag::Box {
            start_tick,
            start_key,
            current_tick,
            current_key,
        }) = self.drag.take()
        else {
            return;
        };
        self.box_select(
            document,
            index,
            track,
            start_tick,
            current_tick,
            start_key,
            current_key,
            additive,
        );
    }
}
