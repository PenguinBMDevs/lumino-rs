//! 音符选择与删除（自 `notes.rs` 拆分，保持各文件 < 400 行）

use super::super::super::selection_set::SelectionSet;

use super::EditorData;
use super::editing::push_merged_remove_events;

impl EditorData {
    /// 通过索引删除单个音符（直接操作 document 当前轨）
    pub fn delete_note_by_index(&mut self, index: usize) {
        if self.remove_note(self.current_track, index).is_some() {
            self.push_history();
            self.mark_current_track_changed();
        }
    }

    /// 批量删除选中音符
    ///
    /// 索引降序逐个删除 document 当前轨（唯一权威源），避免索引漂移；
    /// 删除完成后将待删索引合并为连续区间，统一下发 `RemoveAt { index, count }`
    /// 增量事件（按降序），替代原每音符一条 `RemoveAt { count: 1 }`：
    /// GPU 段内左移次数从 K（选中数）降至「连续删除段数」，整轨框选等海量选中
    /// 场景下避免 K 次整段搬移的全量级开销。
    /// 全程走 `note_delta_events` 增量通道，`note_delta_dirty` 不置位，
    /// 渲染层不触发全量兜底重建（不走全量重建）。
    pub fn delete_selected_notes(&mut self, selected: &SelectionSet) {
        if selected.is_empty() {
            return;
        }
        let current_track = self.current_track;
        // 待删索引降序排列（避免删除后索引漂移）
        let mut sorted: Vec<usize> = selected.iter().copied().collect();
        sorted.sort_unstable_by(|a, b| b.cmp(a));

        // 先从 document 按降序删除（authoritative 源），不经 `remove_note` 以避免逐音符
        // 记录事件；增量事件在删除完成后统一合并下发。
        let mut deleted = 0usize;
        if let Some(doc) = self.document.as_mut() {
            for &idx in &sorted {
                if doc.remove_note(current_track, idx).is_some() {
                    deleted += 1;
                }
            }
        }
        if deleted == 0 {
            return;
        }

        // 仅当前音轨变更需记录段内增量（GPU buffer 布局 = 全量轨段）；
        // 其他轨变化由洋葱皮层 Delta 通道同步，主音轨增量路径无需事件。
        if current_track == self.current_track {
            push_merged_remove_events(&mut self.note_delta_events, &sorted);
        }

        self.push_history();
        self.mark_current_track_changed();
    }

    /// 返回所有音符索引
    pub fn select_all_notes(&self) -> SelectionSet {
        (0..self.current_track_note_count()).collect()
    }

    /// 计算选择框内的音符索引（委托到 get_notes_in_selection_box，消除重复逻辑）
    pub fn compute_selection(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> SelectionSet {
        self.get_notes_in_selection_box(start_tick, start_key, current_tick, current_key)
            .into_iter()
            .collect()
    }

    /// 获取选择框内的音符索引列表
    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        let ts = start_tick.min(current_tick);
        let te = start_tick.max(current_tick);
        let km = start_key.min(current_key);
        let kx = start_key.max(current_key);
        let mut results = Vec::new();
        for (note_idx, note) in self.current_track_notes().iter().enumerate() {
            let tick = note.start_tick as f32;
            let ne = note.end_tick as f32;
            if note.key as u16 >= km && note.key as u16 <= kx && tick <= te && ne >= ts {
                results.push(note_idx);
            }
        }
        results
    }
}
