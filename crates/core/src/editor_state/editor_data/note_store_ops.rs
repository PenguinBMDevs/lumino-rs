//! NoteStore 集成操作：同步、批量移动、批量删除、批量插入
//!
//! 当音符数超过 `NOTE_STORE_THRESHOLD` 时自动启用 NoteStore 作为批量操作热路径。
//! `notes` (im::Vector) 仍为权威源，操作完成后通过 `sync_note_store()` 同步。

use std::collections::HashSet;

use bit_vec::BitVec;

use super::EditorData;
use super::NOTE_STORE_THRESHOLD;
use crate::DragState;
use crate::note::Note;
use crate::note_store::BitSet;

/// 将 `bit_vec::BitVec` 转换为 `NoteStore::BitSet`
///
/// **块级优化**：用 `blocks()` 获取 u64 块，跳过全 0 块 + `trailing_zeros` 只遍历选中位。
/// 16M 50% 选中 ~12ms（vs 旧实现逐位迭代 ~50ms）。
/// 16M 1% 选中 ~0.01ms（vs 旧实现 ~1ms）。
fn bitvec_to_bitset(bv: &BitVec) -> BitSet {
    let len = bv.len();
    let mut s = BitSet::new(len);
    // bit-vec 0.8 的 blocks() 返回 u64 块迭代器
    for (i, block) in bv.blocks().enumerate() {
        if block == 0 {
            continue; // 跳过全 0 块（64 位）
        }
        let base = i * 64;
        let mut bits = block;
        // trailing_zeros: 只遍历被设置的位
        while bits != 0 {
            let tz = bits.trailing_zeros() as usize;
            let idx = base + tz;
            if idx < len {
                s.set(idx);
            }
            bits &= bits - 1; // 清除已处理位
        }
    }
    s
}

impl EditorData {
    /// 同步 notes → note_store（从 im::Vector 重建 SoA 存储）
    ///
    /// 当音符数超过阈值时自动启用 NoteStore。调用时机：
    /// - MIDI 文件加载后
    /// - 音轨切换后（如果 note_store 已启用）
    /// - 批量操作后保持一致性
    pub fn sync_note_store(&mut self) {
        let count = self.notes.len();
        if count >= NOTE_STORE_THRESHOLD {
            if !self.note_store_enabled {
                tracing::info!(
                    "NoteStore 启用: {} 音符 ≥ 阈值 {}",
                    count,
                    NOTE_STORE_THRESHOLD
                );
            }
            self.note_store = crate::note_store::NoteStore::from_im_vector(&self.notes);
            self.note_store_enabled = true;
        } else if self.note_store_enabled {
            // 音符数降至阈值以下，禁用 NoteStore 释放内存
            self.note_store.clear();
            self.note_store_enabled = false;
            tracing::debug!(
                "NoteStore 禁用: {} 音符 < 阈值 {}",
                count,
                NOTE_STORE_THRESHOLD
            );
        }
    }

    /// 从 note_store 回写到 notes（批量操作后恢复一致性）
    ///
    /// 回写前先 compact() 移除墓碑标记的音符，确保 notes 与 store 一致。
    ///
    /// **优化**：无墓碑时跳过 compact()（O(N) 物理复制），只做 to_im_vector() 顺序扫描。
    pub fn sync_notes_from_store(&mut self) {
        if !self.note_store_enabled {
            return;
        }
        // 有墓碑时才物理压缩，纯移动操作无墓碑则跳过
        if self.note_store.tombstone.any_ones() {
            self.note_store.compact();
        }
        self.notes = self.note_store.to_im_vector();
    }

    /// 批量移动选中音符（NoteStore 并行热路径）
    ///
    /// 当 NoteStore 启用时走 `batch_move_parallel`（8 线程并行，16M 50% 18ms），
    /// 否则回退到 `DragState::apply_to_notes`（单线程遍历 BitVec）。
    ///
    /// 返回实际修改的音符数。调用方需在调用前 `push_history()`。
    ///
    /// **注意**：此方法会同步 im::Vector（`sync_notes_from_store()` + `sync_track_notes()`）。
    /// 热路径调用方应优先使用 `batch_move_notes_no_sync()` 避免 O(N) 同步开销。
    pub fn batch_move_notes(
        &mut self,
        selected: &BitSet,
        delta_tick: f32,
        delta_key: i16,
        max_key: u16,
    ) -> usize {
        if selected.count_ones() == 0 {
            return 0;
        }

        if self.note_store_enabled {
            // 热路径：NoteStore 并行批量移动
            let modified = self
                .note_store
                .batch_move_parallel(selected, delta_tick, delta_key, max_key);

            // 回写到 notes 保持一致性
            self.sync_notes_from_store();
            self.sync_track_notes();
            tracing::debug!(
                "NoteStore 批量移动: 修改 {} 音符, 选中 {}",
                modified,
                selected.count_ones()
            );
            modified
        } else {
            // 冷路径：直接遍历 notes
            let mut modified = 0usize;
            for i in 0..self.notes.len() {
                if selected.get(i)
                    && let Some(note) = self.notes.get_mut(i)
                {
                    let new_tick = (note.tick + delta_tick).max(0.0);
                    let new_key =
                        (note.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u16;
                    if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                        note.tick = new_tick;
                        note.key = new_key;
                        modified += 1;
                    }
                }
            }
            if modified > 0 {
                self.sync_track_notes();
            }
            modified
        }
    }

    /// 批量移动选中音符——**不同步 im::Vector**（NoteStore 热路径专用）
    ///
    /// 与 `batch_move_notes` 的区别：
    /// - 不调用 `sync_notes_from_store()`（跳过 O(N) to_im_vector）
    /// - 不调用 `sync_track_notes()`
    /// - 调用方必须在**渲染前**手动调用 `sync_notes_from_store()` 确保一致性
    ///
    /// 适用场景：`commit_pending_drag` 等高频热路径，后续会触发重渲染，
    /// 重渲染时通过 `for_each_note_view` 直接从 NoteStore 读取，无需 im::Vector。
    ///
    /// 返回实际修改的音符数。
    pub fn batch_move_notes_no_sync(
        &mut self,
        selected: &BitSet,
        delta_tick: f32,
        delta_key: i16,
        max_key: u16,
    ) -> usize {
        if selected.count_ones() == 0 {
            return 0;
        }

        if self.note_store_enabled {
            self.note_store
                .batch_move_parallel(selected, delta_tick, delta_key, max_key)
        } else {
            let mut modified = 0usize;
            for i in 0..self.notes.len() {
                if selected.get(i)
                    && let Some(note) = self.notes.get_mut(i)
                {
                    let new_tick = (note.tick + delta_tick).max(0.0);
                    let new_key =
                        (note.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u16;
                    if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                        note.tick = new_tick;
                        note.key = new_key;
                        modified += 1;
                    }
                }
            }
            modified
        }
    }

    /// 批量删除选中音符（NoteStore O(N) 单次遍历）
    ///
    /// 返回删除的音符数。调用方需在调用前 `push_history()`。
    pub fn batch_delete_notes(&mut self, selected: &BitSet) -> usize {
        if selected.count_ones() == 0 {
            return 0;
        }

        let deleted = if self.note_store_enabled {
            let d = self.note_store.delete_selected(selected);
            self.sync_notes_from_store();
            d
        } else {
            // 冷路径：用 HashSet + retain
            let indices: HashSet<usize> =
                (0..self.notes.len()).filter(|&i| selected.get(i)).collect();
            let before = self.notes.len();
            let mut idx = 0usize;
            self.notes.retain(|_| {
                let keep = !indices.contains(&idx);
                idx += 1;
                keep
            });
            before - self.notes.len()
        };

        if deleted > 0 {
            self.sync_track_notes();
        }
        deleted
    }

    /// 批量插入音符（NoteStore 无 realloc 热路径）
    ///
    /// 返回插入的音符数。调用方需在调用前 `push_history()`。
    pub fn batch_insert_notes(&mut self, notes: &[Note]) -> usize {
        if notes.is_empty() {
            return 0;
        }

        let inserted = if self.note_store_enabled {
            let n = self.note_store.insert_bulk(notes);
            self.sync_notes_from_store();
            n
        } else {
            for note in notes {
                self.notes.push_back(note.clone());
            }
            notes.len()
        };

        self.sync_track_notes();
        inserted
    }

    /// 从 DragState 批量移动选中音符（集成层适配）
    ///
    /// 把 `DragState.selected` (BitVec) 转为 `BitSet`，
    /// 然后走 `batch_move_notes` 热路径。返回修改的音符数。
    /// 调用方需在调用前 `push_history()`。
    pub fn batch_move_notes_from_drag_state(
        &mut self,
        drag_state: &DragState,
        max_key: u16,
    ) -> usize {
        if drag_state.is_delta_zero() || !drag_state.has_selection() {
            return 0;
        }
        let bitset = bitvec_to_bitset(&drag_state.selected);
        self.batch_move_notes(
            &bitset,
            drag_state.delta_tick as f32,
            drag_state.delta_key,
            max_key,
        )
    }

    /// 从 DragState 批量移动选中音符——**不同步 im::Vector**
    ///
    /// 与 `batch_move_notes_from_drag_state` 的区别：
    /// 底层走 `batch_move_notes_no_sync`，跳过 `sync_notes_from_store()`。
    ///
    /// 适用场景：`commit_pending_drag` 等高频热路径。调用方需在渲染前
    /// 手动调用 `sync_notes_from_store()` 确保 im::Vector 一致性。
    pub fn batch_move_notes_from_drag_state_no_sync(
        &mut self,
        drag_state: &DragState,
        max_key: u16,
    ) -> usize {
        if drag_state.is_delta_zero() || !drag_state.has_selection() {
            return 0;
        }
        let bitset = bitvec_to_bitset(&drag_state.selected);
        self.batch_move_notes_no_sync(
            &bitset,
            drag_state.delta_tick as f32,
            drag_state.delta_key,
            max_key,
        )
    }

    /// 从 DragState 批量移动选中音符——**直接接受 &BitVec，消除 BitVec→BitSet 转换**
    ///
    /// 底层直接走 `batch_move_parallel_from_bitvec`，跳过 `bitvec_to_bitset` 转换。
    /// 与 `batch_move_notes_from_drag_state_no_sync` 功能等价，但省去 16M 50% 的 ~12ms 转换开销。
    ///
    /// 适用场景：`commit_pending_drag` 等高频热路径。
    pub fn batch_move_notes_from_bitvec_no_sync(
        &mut self,
        drag_state: &DragState,
        max_key: u16,
    ) -> usize {
        if drag_state.is_delta_zero() || !drag_state.has_selection() {
            return 0;
        }
        if self.note_store_enabled {
            self.note_store.batch_move_parallel_from_bitvec(
                &drag_state.selected,
                drag_state.delta_tick as f32,
                drag_state.delta_key,
                max_key,
            )
        } else {
            let mut modified = 0usize;
            for (i, selected) in drag_state.selected.iter().enumerate() {
                if !selected || i >= self.notes.len() {
                    continue;
                }
                if let Some(note) = self.notes.get_mut(i) {
                    let new_tick = (note.tick + drag_state.delta_tick as f32).max(0.0);
                    let new_key = (note.key as i32 + drag_state.delta_key as i32)
                        .clamp(0, max_key as i32) as u16;
                    if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                        note.tick = new_tick;
                        note.key = new_key;
                        modified += 1;
                    }
                }
            }
            modified
        }
    }

    /// 从 HashSet 批量删除选中音符（集成层适配）
    ///
    /// 把 `HashSet<usize>` 转为 `BitSet`，
    /// 然后走 `batch_delete_notes` 热路径。返回删除的音符数。
    /// 调用方需在调用前 `push_history()`。
    pub fn batch_delete_notes_from_set(&mut self, selected: &HashSet<usize>) -> usize {
        if selected.is_empty() {
            return 0;
        }
        let bitset = BitSet::from_iter(self.notes.len(), selected.iter().copied());
        self.batch_delete_notes(&bitset)
    }

    /// 单个音符追加（NoteStore 启用时同步到 note_store，避免后续全量重建）
    ///
    /// 返回插入的音符数（0 或 1）。调用方需在调用前 `push_history()`。
    pub fn push_note(&mut self, note: Note) -> usize {
        if self.note_store_enabled {
            self.note_store.push_back(note.clone());
            self.notes.push_back(note);
            self.sync_track_notes();
            1
        } else {
            self.notes.push_back(note);
            self.sync_track_notes();
            1
        }
    }

    /// 检查 NoteStore 是否启用
    pub fn is_note_store_enabled(&self) -> bool {
        self.note_store_enabled
    }

    /// 计算所有音符的边界（单次顺序扫描 NoteStore，避免 16M 次二分查找）
    pub fn compute_all_notes_bounds(&self) -> (f32, f32, u16, u16) {
        self.note_store.compute_bounds()
    }

    /// 获取音符只读视图（NoteStore 启用时走零 clone 路径）
    ///
    /// 调用方优先使用此方法替代 `notes.get(idx)`，避免 16M 音符场景下
    /// 的 Note 结构体 clone 开销。NoteView 是 Copy 语义，零成本传递。
    ///
    /// NoteStore 未启用时，从 im::Vector 取出 &Note 后零 clone 转 NoteView
    /// （通过 `From<&Note>` 实现，字段全部 Copy）。
    pub fn get_note_view(&self, idx: usize) -> Option<crate::note_store::NoteView> {
        if self.note_store_enabled {
            self.note_store.get_ref(idx)
        } else {
            self.notes.get(idx).map(Into::into)
        }
    }

    /// 遍历所有音符的 NoteView（NoteStore 启用时零 clone）
    ///
    /// 用于 hot path 替代 `notes.iter().enumerate()`，避免每个音符一次 Note clone。
    /// - NoteStore 路径：直接遍历 SoA 数组构造 NoteView（Copy 语义）。
    /// - im::Vector 路径：从 &Note 零 clone 构造 NoteView（通过 `From<&Note>`）。
    pub fn for_each_note_view(&self, mut f: impl FnMut(usize, crate::note_store::NoteView)) {
        if self.note_store_enabled {
            self.note_store.for_each_ref(f);
        } else {
            for (i, n) in self.notes.iter().enumerate() {
                f(i, n.into());
            }
        }
    }

    /// NoteStore 内存占用（MB）
    pub fn note_store_memory_mb(&self) -> f64 {
        if self.note_store_enabled {
            self.note_store.memory_mb()
        } else {
            0.0
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_note_store_auto_enable() {
        let mut data = EditorData::new();
        // 低于阈值：不启用
        for i in 0..100 {
            data.notes.push_back(Note::new(i as f32, 60, 1.0));
        }
        data.sync_note_store();
        assert!(!data.is_note_store_enabled());

        // 超过阈值：启用
        for i in 0..NOTE_STORE_THRESHOLD {
            data.notes.push_back(Note::new(i as f32, 60, 1.0));
        }
        data.sync_note_store();
        assert!(data.is_note_store_enabled());
        assert_eq!(data.note_store.len(), data.notes.len());
    }

    #[test]
    fn test_batch_move_cold_path() {
        let mut data = EditorData::new();
        data.current_track = 1;
        for i in 0..5 {
            data.notes.push_back(Note::new(i as f32 * 10.0, 60, 1.0));
        }
        data.sync_track_notes();

        let mut sel = BitSet::new(5);
        sel.set(0);
        sel.set(2);

        let modified = data.batch_move_notes(&sel, 10.0, 3, 127);
        assert_eq!(modified, 2);
        assert_eq!(data.notes[0].tick, 10.0);
        assert_eq!(data.notes[0].key, 63);
        assert_eq!(data.notes[1].tick, 10.0, "未选中不变");
        assert_eq!(data.notes[2].tick, 30.0);
    }

    #[test]
    fn test_batch_move_hot_path() {
        let mut data = EditorData::new();
        data.current_track = 1;
        for i in 0..NOTE_STORE_THRESHOLD + 100 {
            data.notes.push_back(Note::new(i as f32, 60, 1.0));
        }
        data.sync_note_store();
        assert!(data.is_note_store_enabled());

        let mut sel = BitSet::new(data.notes.len());
        for i in (0..data.notes.len()).step_by(2) {
            sel.set(i);
        }

        let modified = data.batch_move_notes(&sel, 5.0, 2, 127);
        assert_eq!(modified, (data.notes.len() + 1) / 2);

        // 验证一致性：notes 与 note_store 同步
        assert_eq!(data.notes.len(), data.note_store.len());
        assert_eq!(data.notes[0].tick, 5.0);
        assert_eq!(data.notes[1].tick, 1.0, "未选中不变");
    }

    #[test]
    fn test_batch_delete() {
        let mut data = EditorData::new();
        data.current_track = 1;
        for i in 0..10 {
            data.notes.push_back(Note::new(i as f32 * 10.0, 60, 1.0));
        }
        data.sync_track_notes();

        let mut sel = BitSet::new(10);
        sel.set(2);
        sel.set(5);
        sel.set(8);

        let deleted = data.batch_delete_notes(&sel);
        assert_eq!(deleted, 3);
        assert_eq!(data.notes.len(), 7);
        // 保留: 0,1,3,4,6,7,9
        assert_eq!(data.notes[0].tick, 0.0);
        assert_eq!(data.notes[1].tick, 10.0);
        assert_eq!(data.notes[2].tick, 30.0);
    }

    #[test]
    fn test_batch_insert() {
        let mut data = EditorData::new();
        data.current_track = 1;
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        data.sync_track_notes();

        let new_notes = vec![
            Note::new(100.0, 62, 2.0),
            Note::new(200.0, 64, 3.0),
            Note::new(300.0, 66, 4.0),
        ];

        let inserted = data.batch_insert_notes(&new_notes);
        assert_eq!(inserted, 3);
        assert_eq!(data.notes.len(), 4);
        assert_eq!(data.notes[1].tick, 100.0);
        assert_eq!(data.notes[3].tick, 300.0);
    }

    #[test]
    fn test_consistency_after_operations() {
        // 端到端一致性测试：批量移动 + 删除 + 插入后 notes 与 note_store 同步
        let mut data = EditorData::new();
        data.current_track = 1;
        for i in 0..NOTE_STORE_THRESHOLD + 50 {
            data.notes
                .push_back(Note::new(i as f32, 60 + (i % 12) as u16, 1.0));
        }
        data.sync_note_store();
        assert!(data.is_note_store_enabled());

        // 1. 批量移动 50%
        let mut sel = BitSet::new(data.notes.len());
        for i in (0..data.notes.len()).step_by(2) {
            sel.set(i);
        }
        let moved = data.batch_move_notes(&sel, 10.0, 3, 127);
        assert!(moved > 0);
        assert_eq!(data.notes.len(), data.note_store.len());

        // 2. 批量删除 25%
        let mut sel_del = BitSet::new(data.notes.len());
        for i in (0..data.notes.len()).step_by(4) {
            sel_del.set(i);
        }
        let before = data.notes.len();
        let deleted = data.batch_delete_notes(&sel_del);
        assert_eq!(deleted, (before + 3) / 4);
        assert_eq!(data.notes.len(), data.note_store.len());

        // 3. 批量插入 100 个
        let new_notes: Vec<Note> = (0..100)
            .map(|i| Note::new(i as f32 * 5.0, 70, 2.0))
            .collect();
        let before_len = data.notes.len();
        let inserted = data.batch_insert_notes(&new_notes);
        assert_eq!(inserted, 100);
        assert_eq!(data.notes.len(), before_len + 100);
        assert_eq!(data.notes.len(), data.note_store.len());
    }
}
