//! 音符与工程数据写入访问器（含增量事件记录）
//!
//! 由 `accessors.rs` 拆分而来。

use super::super::REORDER_SPAN_CHUNK;
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
    /// 需要获取插入结果时用 [`Self::insert_note_with_id`]。
    pub fn insert_note(&mut self, track_id: usize, note: Note) -> bool {
        self.insert_note_with_id(track_id, note).is_some()
    }

    /// 兼容旧分配器接口（去 ID 后为无操作保留，供历史测试调用）。
    ///
    /// 去 ID 后音符按值引用，无需抬升分配器，本方法为空操作。
    pub fn ensure_note_id_above(&mut self, _id: u64) {}

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
    ///
    /// 仅在实际删除成功后记录 `NoteDeltaEvent::RemoveAt`（避免越界索引
    /// 产生渲染侧错误删除事件）；调用方需在调用前 `push_history()`。
    pub fn remove_note(&mut self, track_id: usize, index: usize) -> Option<NoteEvent> {
        self.modified = true;
        let removed = self
            .document
            .as_mut()
            .and_then(|doc| doc.remove_note(track_id, index))?;
        if track_id == self.current_track {
            self.note_delta_events
                .push(NoteDeltaEvent::RemoveAt { index, count: 1 });
        }
        Some(removed)
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

    /// 就地修改当前轨若干音符（如量化/拖动）后恢复「按 start_tick 升序」不变式，
    /// 并在发生重排时发出**受影响索引区间的增量更新事件**（替代全量重建）。
    ///
    /// 返回 `true` 表示发生重排（已增量同步，调用方**不需要**置 `note_delta_dirty`）。
    /// 直接 `track_notes_mut` 改 tick 会破坏二分查询依赖的排序，破坏后
    /// `window_range`/`position_of_id` 会漏检音符（渲染/命中失效）。
    ///
    /// 区间契约见 [`SortedRestoreRanges`]：各区间外内容与重排前逐位一致 →
    /// 区间 `UpdateRange` 更新完备（无全轨/全工程重传）。
    pub fn restore_current_track_sorted_incremental(&mut self, moved: &[usize]) -> bool {
        let track_id = self.current_track;
        let Some(ranges) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(track_id))
            .and_then(|t| t.restore_sorted_ranges(moved))
        else {
            return false;
        };
        self.push_reorder_ranges_events(&ranges);
        true
    }

    /// 重排受影响区间集合 → 分块 `UpdateRange` 事件（替代全量重建）。
    ///
    /// 每个区间内部再按 [`REORDER_SPAN_CHUNK`] 分块（16MB/块，与渲染侧设备内
    /// 搬移块一致），避免超大单消息与瞬时内存峰值。
    pub fn push_reorder_ranges_events(&mut self, ranges: &[(usize, usize)]) {
        for &(lo, hi) in ranges {
            #[cfg(debug_assertions)]
            self.debug_assert_track_sorted_around(&[lo, hi]);
            let mut start = lo;
            loop {
                let end = (start + REORDER_SPAN_CHUNK - 1).min(hi);
                self.push_update_range(start, end);
                if end >= hi {
                    break;
                }
                start = end + 1;
            }
        }
        self.mark_current_track_changed();
        self.note_delta_dirty = false;
        // 当前音轨由事件通道同步 → 洋葱皮层豁免本轨重建（与 record_update_ranges 一致）
        self.onion_dirty_tracks = Some(HashSet::new());
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
        #[cfg(debug_assertions)]
        self.debug_assert_track_sorted_around(&sorted);
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

    /// 调试断言：验证当前轨在 `indices` 邻域满足「按 start_tick 升序」不变式。
    ///
    /// 就地改 tick 的写路径若忘记 `restore_sorted`，`window_range`/`position_of_id`
    /// 二分查询会漏检音符（渲染可见性/命中检测失效）——在事件记录点设防，
    /// 让任何新写路径的漏排在测试中立即暴露（仅 debug 构建，O(k log 块数)）。
    #[cfg(debug_assertions)]
    fn debug_assert_track_sorted_around(&self, indices: &[usize]) {
        let track = self.current_track_notes();
        let len = track.len();
        for &i in indices {
            if i >= len {
                continue;
            }
            let cur = track.get(i).map(|n| n.start_tick);
            if let Some(prev) = i
                .checked_sub(1)
                .and_then(|p| track.get(p))
                .map(|n| n.start_tick)
                && let Some(c) = cur
            {
                debug_assert!(
                    prev <= c,
                    "轨道失序：索引 {i} 前驱 tick {prev} > 当前 tick {c}（就地改 tick 后必须 restore_sorted）"
                );
            }
            if i + 1 < len
                && let Some(c) = cur
                && let Some(next) = track.get(i + 1).map(|n| n.start_tick)
            {
                debug_assert!(
                    c <= next,
                    "轨道失序：索引 {i} tick {c} > 后继 tick {next}（就地改 tick 后必须 restore_sorted）"
                );
            }
        }
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
