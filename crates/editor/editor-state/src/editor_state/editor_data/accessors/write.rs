//! 音符与工程数据写入访问器（含增量事件记录）
//!
//! 由 `accessors.rs` 拆分而来。

use super::*;

impl EditorData {
    /// 整体替换拍号变化列表并同步到 document（工程设置 / undo 恢复 / 加载）
    ///
    /// 与 [`Self::set_tempo_points`] 同构：`time_signatures` 为编辑态权威源，
    /// `document.time_signatures` 为权威镜像，保证保存/导出链路读到最新值
    /// （消除"UI 改拍号 → 保存丢失"的脆弱补救模式）。
    ///
    /// 调用方需保证输入已按 tick 排序（加载路径天然有序；工程设置路径已排序）。
    pub fn set_time_signatures(&mut self, time_signatures: Vec<(u32, u8, u8)>) {
        self.time_signatures = time_signatures;
        self.modified = true;
        if let Some(doc) = self.document.as_mut() {
            doc.time_signatures = self.time_signatures.clone();
        }
    }

    // ── 音符写入（document 唯一权威） ─────────────────────────

    /// 在指定音轨按 start_tick 升序插入音符，返回分配/保留的全局唯一 id。
    ///
    /// 与 [`Self::insert_note`] 同路径；额外回传 id 供 CreateOp 记录真实身份
    /// （redo 时原样重插，保证 undo/redo 往返 id 稳定）。音轨不存在返回 `None`。
    /// 当前音轨插入会记录 `NoteDeltaEvent::InsertAt`，供 GPU 主音轨段内增量同步。
    pub fn insert_note_with_id(&mut self, track_id: usize, note: Note) -> Option<u64> {
        let doc = self.document.as_mut()?;
        self.modified = true;
        let event = note_to_event(note.clone());
        let start_tick = event.start_tick;
        let id = doc.insert_note_with_id(track_id, event)?;
        if track_id == self.current_track {
            let track = doc.track_notes(track_id);
            // 注意：上方 `doc.insert_note_with_id` 已把新音符按升序插入文档，
            // 故此处 `track` 已包含该音符本身。`partition_point(start_tick + 1)`
            // 返回 tick < start_tick + 1（即 tick <= start_tick）的音符数，
            // 其中含新音符自身，故减 1 得到新音符的文档索引（GPU 段内布局保序）。
            let index = if start_tick == u32::MAX {
                track.len().saturating_sub(1)
            } else {
                track
                    .partition_point(start_tick.saturating_add(1))
                    .saturating_sub(1)
            };
            self.note_delta_events
                .push(NoteDeltaEvent::InsertAt { index, note });
        }
        Some(id)
    }

    /// 在指定音轨按 start_tick 升序插入音符（f32 tick 无损转换写回）。
    ///
    /// 返回是否插入成功（音轨不存在返回 false）。调用方需在调用前 `push_history()`。
    /// 当前音轨插入会记录 `NoteDeltaEvent::InsertAt`，供 GPU 主音轨段内增量同步。
    /// 需要获取分配到的 id 时用 [`Self::insert_note_with_id`]。
    pub fn insert_note(&mut self, track_id: usize, note: Note) -> bool {
        self.insert_note_with_id(track_id, note).is_some()
    }

    /// 抬升文档级 note id 分配器，确保严格大于 `id`，避免与协作/快照外来 id 碰撞。
    ///
    /// 委托给 `MidiDocument::ensure_note_id_above`；无 document 时静默跳过。
    pub fn ensure_note_id_above(&mut self, id: u64) {
        if let Some(doc) = self.document.as_mut() {
            doc.ensure_note_id_above(id);
        }
    }

    /// 确保指定音轨存在（不存在则自动扩轨，图片转 MIDI 自动建轨用）。
    /// document 为空时返回 false。
    pub fn ensure_track(&mut self, track_id: usize) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        while doc.track_count() <= track_id {
            doc.add_empty_track();
        }
        true
    }

    /// 在指定音轨指定索引处删除音符。返回被删除的音符。
    pub fn remove_note(&mut self, track_id: usize, index: usize) -> Option<NoteEvent> {
        self.modified = true;
        if track_id == self.current_track {
            self.note_delta_events
                .push(NoteDeltaEvent::RemoveAt { index, count: 1 });
        }
        self.document.as_mut()?.remove_note(track_id, index)
    }

    /// 替换指定音轨指定索引处的音符（内部按序重新插入，保持升序不变式）。
    pub fn update_note(&mut self, track_id: usize, index: usize, note: Note) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        self.modified = true;
        let event = note_to_event(note.clone());
        let start_tick = event.start_tick;
        if !doc.update_note(track_id, index, event) {
            return false;
        }
        if track_id == self.current_track {
            let track = doc.track_notes(track_id);
            let new_index = if start_tick == u32::MAX {
                track.len().saturating_sub(1)
            } else {
                track
                    .partition_point(start_tick.saturating_add(1))
                    .saturating_sub(1)
            };
            // update 语义 = 删除旧位置 + 按新 tick 插入新位置
            self.note_delta_events
                .push(NoteDeltaEvent::RemoveAt { index, count: 1 });
            self.note_delta_events.push(NoteDeltaEvent::InsertAt {
                index: new_index,
                note,
            });
        }
        true
    }

    /// 整轨替换音符（undo/redo 快照恢复专用）。
    pub fn replace_track_notes(&mut self, track_id: usize, notes: Vec<NoteEvent>) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        self.modified = true;
        doc.replace_track_notes(track_id, notes)
    }

    /// 整轨替换音符（undo/redo 快照恢复专用，O(块数) 浅拷贝版）。
    ///
    /// 直接共享快照块 Arc，不做数据复制（1600W 音符工程 undo/redo 免整轨拷贝）。
    pub fn replace_track_notes_chunked(
        &mut self,
        track_id: usize,
        notes: &lumino_midi_model::ChunkedList<NoteEvent>,
    ) -> bool {
        let Some(doc) = self.document.as_mut() else {
            return false;
        };
        self.modified = true;
        doc.replace_track_notes_chunked(track_id, notes)
    }

    // ── 增量事件记录 ─────────────────────────────────────────

    /// 就地修改当前轨若干音符（如量化）后恢复「按 start_tick 升序」不变式。
    ///
    /// 返回 `true` 表示发生重排（调用方须走主轨全量重建：区间事件按旧索引失效）。
    /// 直接 `track_notes_mut` 改 tick 会破坏二分查询依赖的排序，破坏后
    /// `window_range`/`position_of_id` 会漏检音符（渲染/命中失效）。
    pub fn restore_current_track_sorted(&mut self, moved: &[usize]) -> bool {
        let track = self.current_track;
        self.document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(track))
            .is_some_and(|t| t.restore_sorted(moved))
    }

    /// 记录等长修改增量事件（整轨同步版）
    ///
    /// 将 `indices`（修改的 notes 索引，无序可重复）合并为连续区间
    /// `UpdateRange` 事件，随后标记变化并清除 dirty（事件已完整记录）。
    ///
    /// 等长修改由 `note_delta_events` (UpdateMany) 同步当前音轨段，
    /// 因此清空 `onion_dirty_tracks` 避免洋葱皮层再发一次 `TrackDelta`。
    pub fn record_update_ranges(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        self.push_update_range_events(indices);
        self.mark_current_track_changed();
        self.note_delta_dirty = false;
        // 等长修改已记录为段内 UpdateMany，当前音轨由事件通道同步，
        // 不需要洋葱皮层再发 TrackDelta（避免拖动热路径每帧重传整轨）。
        self.onion_dirty_tracks = Some(HashSet::new());
    }

    /// 记录等长修改增量事件（流式同步版，拖动热路径）
    ///
    /// 与 [`Self::record_update_ranges`] 相同的事件记录，供
    /// `apply_drag_state_streaming` 使用。
    pub fn record_update_ranges_streamed(&mut self, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        self.push_update_range_events(indices);
        self.mark_current_track_changed();
        self.note_delta_dirty = false;
        self.onion_dirty_tracks = Some(HashSet::new());
    }

    /// 将升序去重后的索引合并为连续区间事件（纯数据操作，不同步）
    fn push_update_range_events(&mut self, indices: &[usize]) {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.is_empty() {
            return;
        }
        let mut start = sorted[0];
        let mut prev = sorted[0];
        for &i in &sorted[1..] {
            if i == prev + 1 {
                prev = i;
                continue;
            }
            self.push_update_range(start, prev);
            start = i;
            prev = i;
        }
        self.push_update_range(start, prev);
    }

    /// 推送单个连续区间事件（越界索引防御性过滤）
    fn push_update_range(&mut self, start: usize, end: usize) {
        let notes: Vec<Note> = self
            .current_track_notes()
            .iter()
            .skip(start)
            .take(end - start + 1)
            .map(event_to_note)
            .collect();
        if !notes.is_empty() {
            self.note_delta_events.push(
                crate::editor_state::editor_data::NoteDeltaEvent::UpdateRange {
                    start_index: start,
                    notes,
                },
            );
        }
    }
}
