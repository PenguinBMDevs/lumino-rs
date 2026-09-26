//! 历史操作应用（MoveOp / CreateOp）与 DragState 构造
//!
//! 这些逻辑从 `history.rs` 拆分出来，使主文件保持在 400 行以内。
//!
//! 2026-09 去 ID：历史条目一律按值引用（`position_of` 窗口定位，
//! O(log N + 同 tick 数)，无全扫兜底），移动抽象为“删旧 + 加新”。

use std::collections::{HashMap, HashSet};

use lumino_midi_model::NoteEvent;
use lumino_note_core::Note;
use lumino_note_core::history::{CreateOp, MoveOp};

use super::EditorData;
use crate::DragState;

impl EditorData {
    /// 应用音符创建日志到 document（增量恢复，单一权威源）
    ///
    /// - `inverse=true`（undo）：按值精确定位删除（窗口二分，无全扫）。
    ///   顺序无关——不受同轨后续操作导致的索引漂移影响；同值多份按份数删。
    /// - `inverse=false`（redo）：按 tick 有序重新插入（按值）。
    ///
    /// 返回实际处理的音符数。
    pub fn apply_create_ops(&mut self, ops: &[CreateOp], inverse: bool) -> usize {
        if ops.is_empty() {
            return 0;
        }
        let mut count = 0usize;
        for op in ops {
            let track_id = op.track_id as usize;
            if inverse {
                let Some(idx) = self.locate_create_op_note(track_id, op) else {
                    continue;
                };
                if self.remove_note(track_id, idx).is_some() {
                    count += 1;
                }
            } else {
                // redo：按 tick 有序重新插入（按值）。
                if self.insert_note(track_id, super::super::accessors::event_to_note(&op.note)) {
                    count += 1;
                }
            }
        }
        count
    }

    /// 定位 CreateOp 对应音符的当前索引（按值窗口定位，无全扫）。
    ///
    /// 经 `ChunkedList::position_of` 二分同 tick 段全字段匹配；
    /// 未命中返回 None（同值多份取首个，份数语义正确）。
    fn locate_create_op_note(&self, track_id: usize, op: &CreateOp) -> Option<usize> {
        let track = self.track_notes(track_id);
        track.position_of(&op.note)
    }

    /// 应用 MoveOp 列表到 document 对应音轨（单一权威源，删加语义）。
    ///
    /// 每个被移动音符按**值**定位（`position_of` 窗口二分，无全扫兜底），
    /// 不依赖提交时的索引——协作远端插入/删除导致的索引漂移不再损坏无关音符。
    /// 移动抽象为“删旧 + 加新”（长度不变，clamp 按 `max_key`）：
    /// 前向删 `originals`、加 `moved`；`inverse=true`（undo）删 `moved`、加回 `originals`。
    /// 同值多份按份数处理（逐个定位即删，删一即删任一，集合语义等价）。
    ///
    /// `max_key` 用于 clamp key 范围（通常传 `visible_key_count - 1`）。
    /// 返回实际处理的音符数（删 + 加计一次移动，按删除份数计）。
    pub fn apply_move_ops(&mut self, ops: &[MoveOp], inverse: bool, max_key: u16) -> usize {
        if ops.is_empty() {
            return 0;
        }

        let mut modified = 0usize;
        let mut dirty_tracks: HashSet<usize> = HashSet::new();

        for op in ops {
            let track_id = op.track_id as usize;
            if op.originals.is_empty() {
                continue;
            }
            // 按方向确定删/加集合（op 恒为前向，方向由 inverse 标志决定）。
            let (olds, news): (Vec<NoteEvent>, Vec<NoteEvent>) = if !inverse {
                let news = op.moved_notes(max_key);
                (op.originals.clone(), news)
            } else {
                let olds = op.moved_notes(max_key);
                (olds, op.originals.clone())
            };
            // 逐个按值定位即删（窗口二分，无全扫；每次重查保证同值多份取到不同副本）。
            let mut deleted = 0usize;
            for old in &olds {
                let idx_opt = self
                    .document
                    .as_ref()
                    .map(|doc| doc.track_notes(track_id).position_of(old))
                    .unwrap_or(None);
                let Some(idx) = idx_opt else {
                    continue;
                };
                if self.remove_note(track_id, idx).is_some() {
                    deleted += 1;
                }
            }
            if deleted == 0 {
                continue;
            }
            // 批量归并插入新位置（O(N+M) 单次归并，无逐条扫描）。
            let inserted = self
                .document
                .as_mut()
                .map(|doc| doc.batch_insert_notes(track_id, news))
                .unwrap_or(0);
            modified += deleted.min(inserted.max(deleted));
            dirty_tracks.insert(track_id);
        }

        if modified > 0 {
            // 删加后索引整体位移，不推旧索引 UpdateRange（已失效），
            // 统一按受影响音轨标记（当前轨走 TrackDelta 单轨重建，非全量会话）。
            self.mark_track_notes_changed_for(Some(dirty_tracks));
            // 若当前轨在内，主轨段需重建（置位由 mark_track_notes_changed_for 内部处理，
            // 此处显式保证事件队列不残留旧区间事件）。
            self.note_delta_events.clear();
        }
        modified
    }

    /// 将 `(track_id, index)` 修改集合按轨合并连续区间并推送 `UpdateRange` 事件。
    #[allow(dead_code)]
    fn push_move_update_ranges(&mut self, modified_indices: &[(usize, usize)]) {
        if modified_indices.is_empty() {
            return;
        }
        // 快路径（常见）：单轨且索引严格升序（解析顺序即索引顺序）→
        // 直接扫描生成连续区间，免 HashMap 分组 + Vec 拷贝 + 排序（百万级省 ~30MB）。
        let single_track = modified_indices
            .iter()
            .all(|&(t, _)| t == modified_indices[0].0);
        let sorted_unique = modified_indices.windows(2).all(|w| w[0].1 < w[1].1);
        if single_track && sorted_unique {
            let track_id = modified_indices[0].0;
            let mut start = modified_indices[0].1;
            let mut prev = start;
            for &(_, i) in &modified_indices[1..] {
                if i == prev + 1 {
                    prev = i;
                    continue;
                }
                self.push_update_range_event(track_id, start, prev);
                start = i;
                prev = i;
            }
            self.push_update_range_event(track_id, start, prev);
            return;
        }
        let mut by_track: HashMap<usize, Vec<usize>> = HashMap::new();
        for &(track_id, idx) in modified_indices {
            by_track.entry(track_id).or_default().push(idx);
        }
        for (track_id, mut idxs) in by_track {
            idxs.sort_unstable();
            idxs.dedup();
            let mut start = idxs[0];
            let mut prev = idxs[0];
            for &i in &idxs[1..] {
                if i == prev + 1 {
                    prev = i;
                    continue;
                }
                self.push_update_range_event(track_id, start, prev);
                start = i;
                prev = i;
            }
            self.push_update_range_event(track_id, start, prev);
        }
    }

    /// 推送单段 `[start, end]`（闭区间）的音符增量更新事件。
    ///
    /// 直接经 `iter_window` 顺序取音符转换（免 `get_range` 的 `Vec<&T>` 中间层）。
    fn push_update_range_event(&mut self, track_id: usize, start: usize, end: usize) {
        let mapped: Vec<Note> = match self.document.as_ref() {
            Some(doc) => doc
                .track_notes(track_id)
                .iter_window(start, end + 1)
                .map(|(_, n)| super::super::accessors::event_to_note(n))
                .collect(),
            None => return,
        };
        if !mapped.is_empty() {
            self.note_delta_events.push(
                crate::editor_state::editor_data::NoteDeltaEvent::UpdateRange {
                    start_index: start,
                    notes: mapped,
                },
            );
        }
    }

    /// 当前 view 下可用于 clamp key 的最大 key 索引
    ///
    /// EditorData 本身不持有 view，默认用 255（MIDI 最大 key）。
    /// UI 层调用 `apply_move_ops` 时应传入实际 `visible_key_count - 1`。
    pub(crate) fn max_key_for_move_op(&self) -> u16 {
        255
    }

    /// 从 DragState 构造 MoveOp 列表（按连续区间拆分，按值捕获原始快照 + 移动后快照）。
    ///
    /// **优化**：`selected_indices()` 已按索引升序返回，无需 sort。
    /// 每段捕获完整 `NoteEvent` 值快照 + 按 `max_key` clamp 计算的移动后快照：
    /// 供 undo（删 moved 加回 originals）、redo（删 originals 加 moved）直接使用，
    /// 回放时不再重算，避免 clamp 标准漂移。
    /// 本方法为兼容旧测试保留，默认 `max_key=255`；生产拖动路径请用
    /// [`Self::move_ops_from_drag_state_with_max_key`] 传入视图实际 max_key。
    pub fn move_ops_from_drag_state(&self, drag_state: &DragState) -> Vec<MoveOp> {
        self.move_ops_from_drag_state_with_max_key(drag_state, 255)
    }

    /// 从 DragState 构造 MoveOp 列表（显式 max_key 版，供拖动提交传入视图 clamp）。
    pub fn move_ops_from_drag_state_with_max_key(
        &self,
        drag_state: &DragState,
        max_key: u16,
    ) -> Vec<MoveOp> {
        let track_id = self.current_track as u32;
        let indices: Vec<usize> = drag_state.selected_indices();
        if indices.is_empty() {
            return Vec::new();
        }
        // selected_indices() 已升序，无需 sort

        let delta_tick = SaturatingInto::<i32>::saturating_into(drag_state.delta_tick);
        let delta_key = drag_state.delta_key;

        let mut ops = Vec::new();
        let mut seq = 0u16;
        let mut range_start = indices[0];
        let mut prev = indices[0];

        // 直接遍历 document 当前轨提取完整值快照（单一权威源，按值引用），
        // 并按 max_key 预计算移动后快照（clamp + 长度不变）。
        let track_notes = self.current_track_notes();
        let make_op = |start: usize, end: usize, seq: u16| {
            let mut originals = Vec::with_capacity(end - start + 1);
            if let Some(slice) = track_notes.get_range(start..=end) {
                for note in slice {
                    originals.push(*note);
                }
            }
            let moved: Vec<NoteEvent> = originals
                .iter()
                .map(|n| {
                    let mut m = *n;
                    let new_tick = (n.start_tick as i64 + delta_tick as i64).max(0) as u32;
                    let new_key = (n.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u8;
                    let len = n.end_tick.saturating_sub(n.start_tick).max(1);
                    m.start_tick = new_tick;
                    m.end_tick = new_tick.saturating_add(len);
                    m.key = new_key;
                    m
                })
                .collect();
            MoveOp {
                track_id,
                originals,
                moved,
                delta_tick,
                delta_key,
                seq,
            }
        };

        for &note_idx in &indices[1..] {
            if note_idx == prev + 1 {
                prev = note_idx;
            } else {
                ops.push(make_op(range_start, prev, seq));
                seq = seq.wrapping_add(1);
                range_start = note_idx;
                prev = note_idx;
            }
        }
        // 最后一段
        ops.push(make_op(range_start, prev, seq));
        // 空段（索引越界防御）不进入历史——无值快照即无操作
        ops.retain(|op| !op.originals.is_empty());
        ops
    }
}

/// i64 饱和转换到 i32 的辅助 trait
pub(crate) trait SaturatingInto<T> {
    /// 饱和转换
    fn saturating_into(self) -> T;
}

impl SaturatingInto<i32> for i64 {
    fn saturating_into(self) -> i32 {
        self.clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }
}
