//! 批量插入音符（按值，去 ID）
//!
//! 去 ID 后批量插入按值完成，无需回传 ID 列表；保留 `*_with_ids` 兼容签名
//! （返回占位 0 列表），供粘贴/复制/协作落盘保持批量语义。

use super::super::EditorData;
use lumino_note_core::note::Note;

impl EditorData {
    /// 批量插入（按值），返回占位列表（兼容旧签名）。
    ///
    /// 按值批量归并 O(N+M)，无全轨重扫。仅当前轨触发主轨全量脏标记。
    pub fn batch_insert_notes_with_ids(&mut self, notes: &[Note]) -> Vec<u64> {
        if notes.is_empty() {
            return Vec::new();
        }
        let Some(doc) = self.document.as_mut() else {
            return Vec::new();
        };
        let events: Vec<lumino_midi_model::NoteEvent> = notes
            .iter()
            .map(|n| super::super::accessors::note_to_event(n.clone()))
            .collect();
        let n = events.len();
        let inserted = doc.batch_insert_notes(self.current_track, events);
        if inserted > 0 {
            // 结构性大插入：主轨段重建（TrackDelta），不走全量会话兜底
            self.mark_main_track_struct_changed();
            self.mark_current_track_changed();
        }
        vec![0u64; n]
    }

    /// 批量插入到指定音轨（按值），返回占位列表（兼容旧签名）。
    ///
    /// 仅当 `track_id == current_track` 时触发主轨全量脏标记（其余轨走洋葱皮增量豁免）。
    pub fn batch_insert_notes_to_track_with_ids(
        &mut self,
        track_id: usize,
        notes: &[Note],
    ) -> Vec<u64> {
        if notes.is_empty() {
            return Vec::new();
        }
        let Some(doc) = self.document.as_mut() else {
            return Vec::new();
        };
        let events: Vec<lumino_midi_model::NoteEvent> = notes
            .iter()
            .map(|n| super::super::accessors::note_to_event(n.clone()))
            .collect();
        let n = events.len();
        let inserted = doc.batch_insert_notes(track_id, events);
        if inserted > 0 && track_id == self.current_track {
            self.mark_main_track_struct_changed();
            self.mark_current_track_changed();
        }
        vec![0u64; n]
    }

    /// 批量插入 **NoteEvent**（须按 `start_tick` 升序）到指定音轨（按值，兼容签名）。
    ///
    /// 粘贴热路径专用：解码端直接产出升序 `NoteEvent`（免 `Note` 中间层与二次转换），
    /// 走免排序单次归并插入（O(N+M)）。
    /// **调用方须保证 `events` 已按 `start_tick` 升序**（剪贴板解码天然满足）。
    /// 仅当 `track_id == current_track` 时触发主轨全量脏标记。
    pub fn batch_insert_events_to_track_with_ids(
        &mut self,
        track_id: usize,
        events: Vec<lumino_midi_model::NoteEvent>,
    ) -> Vec<u64> {
        if events.is_empty() {
            return Vec::new();
        }
        let Some(doc) = self.document.as_mut() else {
            return Vec::new();
        };
        let n = events.len();
        let inserted = doc.batch_insert_notes_sorted(track_id, events);
        if inserted > 0 && track_id == self.current_track {
            self.mark_main_track_struct_changed();
            self.mark_current_track_changed();
        }
        vec![0u64; n]
    }

    /// 批量插入 **NoteEvent**（须按 `start_tick` 升序）（按值）。
    ///
    /// 未连接协作时的粘贴路径专用。仅当 `track_id == current_track` 时触发主轨全量脏标记。
    pub fn batch_insert_events_to_track(
        &mut self,
        track_id: usize,
        events: Vec<lumino_midi_model::NoteEvent>,
    ) {
        if events.is_empty() {
            return;
        }
        let Some(doc) = self.document.as_mut() else {
            return;
        };
        let inserted = doc.batch_insert_notes_sorted(track_id, events);
        if inserted > 0 && track_id == self.current_track {
            self.mark_main_track_struct_changed();
            self.mark_current_track_changed();
        }
    }
}
