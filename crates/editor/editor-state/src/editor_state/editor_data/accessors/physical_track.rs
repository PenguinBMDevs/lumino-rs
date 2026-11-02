//! 物理删轨的减轨增量：清空轨道音符，只标记该轨的 tick 区间。

use std::collections::HashSet;

use lumino_midi_model::NoteEvent;

use super::super::EditorData;

impl EditorData {
    /// 永久删除轨道内容（保留轨位，避免 sidebar/协作 id 整体重排）。
    ///
    /// 音符被物理清空，但轨道槽位保留；CPU 洋葱皮只重建这些音符覆盖的
    /// tick 区间，索引走单轨增量，不触发全量密度/索引重建。
    pub fn remove_track_notes_physical(&mut self, track_id: usize) -> bool {
        let Some(document) = self.document.as_ref() else {
            return false;
        };
        if track_id >= document.track_count() {
            return false;
        }
        if document.track_notes(track_id).is_empty() {
            return true;
        }
        let removed: Vec<NoteEvent> = document.track_notes(track_id).to_vec();
        if !self.replace_track_notes(track_id, Vec::new()) {
            return false;
        }
        // replace_track_notes 已经记录脏 tick 区间；这里补音轨级版本号。
        self.mark_track_notes_changed_for(Some(HashSet::from([track_id])));
        // 永久删除不参与撤销；清掉“无原子变更”造成的 full 待挂标记。
        self.history_pending_full = false;
        !removed.is_empty()
    }
}
