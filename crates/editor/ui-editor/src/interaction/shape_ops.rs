//! 形状对象 → 音符：再生成、提交与按值清理（S0-3b）
//!
//! ## 语义
//!
//! 形状是**参数化文档对象**（Spiderweb `notes/fx.py` 的口径）：几何/参数变更多次，
//! 音符按当前参数**重新生成**；旧音符按其**值指纹**清理，避免残留孤儿音符。
//!
//! ## 一次参数提交 = 一次撤销单元
//!
//! 沿用仓库既有配方（`split_note` / `glue_selected_notes`）：
//! `push_history()` 先给**音符**拍快照 → 清理旧音符 → 写入新音符 → 事务标记。
//! **形状参数**侧由 [`ShapeStore`](lumino_editor_state::ShapeStore) 的**同分组日志**回退
//! （撤销/重做入口在 `impls/editor_impl/history.rs`），于是「形状与它生成的音符」
//! 在同一个 group 内同生共死——这就是 S0-3 的验收标准。
//!
//! ## 与曲线工具同源
//!
//! 生成走的是曲线工具确认时的同一条链路（[`geom::flatten_path`] →
//! [`stroke_band::outline_notes`] → [`paths::keep_longest`] → 长度下限），
//! 不另起一套几何——否则「预览/工具生成的音符」与「形状再生成的音符」会不一致。

use crate::interaction::line_tool::{geom, paths, stroke_band};
use crate::{Editor, Note};
use lumino_core::shape::{ShapeModel, ShapeRect};
use lumino_editor_state::shape_ops::{
    NoteOpContext, OpContext, apply_geometry_ops, apply_note_ops,
};
use lumino_editor_state::{BezierAnchor, ShapeNote, note_to_event};
use lumino_midi_model::TickIndexedEvents;

impl Editor {
    /// 打开工程后灌注形状：**只登记，不生成音符**（音符来自工程本身）
    pub fn load_shapes(&mut self, models: Vec<ShapeModel>) {
        self.editor_state.shape_store.load_all(models);
        self.mark_notes_changed();
    }

    /// 可持久化的形状快照（写工程用；隐藏/已删除的不写盘）
    pub fn shapes_snapshot(&self) -> Vec<ShapeModel> {
        self.editor_state.shape_store.persisted_models()
    }

    /// 形状参数 → 音符指纹
    ///
    /// 与曲线工具确认同源：展平 → 逐音高行解析 + 法线加粗 → 同 tick 同 key 留最长 →
    /// 长度下限补齐。`anchors.len() < 2` 视为无音符形状（仍可登记与持久化）。
    pub(crate) fn shape_notes_of(&self, model: &ShapeModel, track: usize) -> Vec<ShapeNote> {
        let anchors: Vec<BezierAnchor> = model
            .geometry
            .anchors
            .iter()
            .map(|a| BezierAnchor {
                pos: a.pos,
                in_handle: a.in_handle,
                out_handle: a.out_handle,
                handles_auto: a.handles_auto,
            })
            .collect();
        if anchors.len() < 2 {
            return Vec::new();
        }
        let poly = geom::flatten_path(&anchors);
        // Geometry 阶段算子（Tumours）：先改路径，再走同源音符生成
        let ctx = OpContext::new(f64::from(self.editor_state.view.ppq));
        let poly = apply_geometry_ops(&poly, &model.ops, &ctx);
        let note_ctx = NoteOpContext {
            glue_box: shape_box_of(&poly),
        };
        let key_count = self.editor_state.view.key_count as i32;
        let min_ticks = paths::min_note_length_ticks(
            self.editor_state.line_tool.min_note_division,
            self.editor_state.view.ppq,
        );
        let raw = stroke_band::outline_notes(
            &poly,
            self.draw_thickness(),
            self.editor_state.view.zoom_x,
            self.editor_state.view.zoom_y,
            key_count,
        );
        let mut notes: Vec<ShapeNote> = paths::keep_longest(&raw)
            .into_iter()
            .filter(|n| n.key >= 0 && n.key < key_count)
            .map(|n| {
                let (tick, key, length) = paths::to_note_triple(&n, min_ticks);
                ShapeNote {
                    track,
                    tick,
                    key,
                    length,
                }
            })
            .collect();
        // Notes 阶段算子（Glue 置顶；DRAW-004..010 后续接入）
        apply_note_ops(&mut notes, &model.ops, &note_ctx);
        notes
    }

    /// 新建形状：生成音符 + 登记形状（一次撤销单元）；Conductor 轨（0）拒绝
    pub fn create_shape(&mut self, model: ShapeModel) -> bool {
        let track = model.track as usize;
        if track == 0 {
            tracing::debug!("形状: Conductor 轨道禁止放置音符");
            return false;
        }
        let notes = self.shape_notes_of(&model, track);
        self.push_history();
        let group = self.current_history_group();
        let written = self.write_shape_notes(track, &notes);
        let id = self.editor_state.shape_store.insert(model, group);
        self.editor_state.shape_store.assign_notes(id, written);
        self.mark_notes_changed();
        true
    }

    /// 修改形状参数：按值清理旧音符 → 按新参数再生成 → 记录分组（一次撤销单元）
    pub fn edit_shape(&mut self, id: u64, new_model: ShapeModel) -> bool {
        if self.editor_state.shape_store.model(id).is_none() {
            return false;
        }
        let track = new_model.track as usize;
        if track == 0 {
            return false;
        }
        let old_notes = self.editor_state.shape_store.notes_of(id).to_vec();
        let new_notes = self.shape_notes_of(&new_model, track);
        self.push_history();
        let group = self.current_history_group();
        self.remove_shape_notes(track, &old_notes);
        let written = self.write_shape_notes(track, &new_notes);
        self.editor_state.shape_store.edit(id, new_model, group);
        self.editor_state.shape_store.assign_notes(id, written);
        self.editor_state.data.mark_current_track_changed();
        self.mark_notes_changed();
        true
    }

    /// 删除形状：清理它的音符 + 标记删除（一次撤销单元）
    pub fn delete_shape(&mut self, id: u64) -> bool {
        let Some(model) = self.editor_state.shape_store.model(id).cloned() else {
            return false;
        };
        let track = model.track as usize;
        let notes = self.editor_state.shape_store.notes_of(id).to_vec();
        self.push_history();
        let group = self.current_history_group();
        self.remove_shape_notes(track, &notes);
        self.editor_state.shape_store.mark_deleted(id, group);
        self.editor_state.shape_store.assign_notes(id, Vec::new());
        self.editor_state.data.mark_current_track_changed();
        self.mark_notes_changed();
        true
    }

    /// 本次提交所属的历史分组 ID（`push_history()` 之后调用）
    fn current_history_group(&self) -> u64 {
        self.editor_state
            .data
            .history
            .undo_back()
            .and_then(crate::interaction::drawn_shape::history_entry_group)
            .unwrap_or(0)
    }

    /// 写入形状音符（小规模逐条 / 大规模批量，口径同曲线工具确认）
    ///
    /// ⚠️ **不调用 `push_note_create`**：撤销单元由 [`Self::create_shape`] /
    /// [`Self::edit_shape`] 里的 `push_history()` 快照承担（与 `split_note` 同款）。
    /// 再压一条 CreateOp 会多出第二条历史条目，破坏「一次提交 = 一次撤销」，
    /// 并让撤销时读到的 group 不是形状日志所记录的那个。
    fn write_shape_notes(&mut self, track: usize, notes: &[ShapeNote]) -> Vec<ShapeNote> {
        if notes.is_empty() {
            return Vec::new();
        }
        self.editor_state.data.ensure_track(track);
        let mut written: Vec<ShapeNote> = Vec::with_capacity(notes.len());
        if notes.len() <= crate::interaction::BATCH_INSERT_THRESHOLD {
            for sn in notes {
                let note = Note::new(sn.tick, sn.key, sn.length);
                if self
                    .editor_state
                    .data
                    .insert_note_with_id(track, note)
                    .is_some()
                {
                    written.push(*sn);
                }
            }
        } else {
            let payload: Vec<Note> = notes
                .iter()
                .map(|sn| Note::new(sn.tick, sn.key, sn.length))
                .collect();
            let before = self.editor_state.data.track_notes(track).len();
            self.editor_state.data.batch_insert_notes_with_ids(&payload);
            if self.editor_state.data.track_notes(track).len() == before {
                return Vec::new();
            }
            written.extend_from_slice(notes);
        }
        self.broadcast_shape_sync(track, &written, true);
        self.editor_state.data.mark_current_track_changed();
        written
    }

    /// 按值指纹清理旧音符（降序删除，避免索引漂移）
    fn remove_shape_notes(&mut self, track: usize, notes: &[ShapeNote]) {
        if notes.is_empty() {
            return;
        }
        let mut used: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut indices: Vec<usize> = Vec::with_capacity(notes.len());
        for sn in notes {
            // 形状音符一律以 `Note::new` 写入（力度 100 / 通道 0），指纹按同口径重建
            let ev = note_to_event(Note::new(sn.tick, sn.key, sn.length));
            if let Some(idx) = self
                .editor_state
                .data
                .track_notes(track)
                .position_of_unused(&ev, &used)
            {
                used.insert(idx);
                indices.push(idx);
            }
        }
        indices.sort_unstable_by(|a, b| b.cmp(a));
        for idx in indices {
            let _ = self.editor_state.data.remove_note(track, idx);
        }
        let removed: Vec<ShapeNote> = notes.to_vec();
        self.broadcast_shape_sync(track, &removed, false);
    }

    /// 形状音符的协作对账（改变音符数量 → 按值广播「删 / 加」）
    ///
    /// 形状对象本身**不同步**（已拍板口径）；这里只同步它产生的音符。
    /// 开关判定在 `EditorData::push_collab_transform_sync` 内部。
    fn broadcast_shape_sync(&mut self, track: usize, notes: &[ShapeNote], is_insert: bool) {
        for sn in notes {
            self.editor_state
                .data
                .push_collab_transform_sync(is_insert, sn.tick, sn.key, sn.length, 100, 0, track);
        }
    }
}

/// 展平路径 → 形状包围盒（key 上下各扩 0.5，供 Glue 的 Boxes 使用）
fn shape_box_of(points: &[(f64, f64)]) -> Option<ShapeRect> {
    let &(first_tick, first_key) = points.first()?;
    let mut tick_lo = first_tick;
    let mut tick_hi = first_tick;
    let mut key_lo = first_key;
    let mut key_hi = first_key;
    for &(tick, key) in points {
        tick_lo = tick_lo.min(tick);
        tick_hi = tick_hi.max(tick);
        key_lo = key_lo.min(key);
        key_hi = key_hi.max(key);
    }
    Some(ShapeRect::normalized(
        tick_lo as f32,
        key_lo as f32 - 0.5,
        tick_hi as f32,
        key_hi as f32 + 0.5,
    ))
}

#[cfg(test)]
mod tests;
