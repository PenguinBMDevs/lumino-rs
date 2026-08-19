//! MidiDocument 音符编辑（从 `document.rs` 拆分）
//!
//! 音符插入/删除/替换/清空，以及 max_end_tick 缓存置脏维护。

use crate::note_event::NoteEvent;

use super::MidiDocument;

impl MidiDocument {
    /// 追加一条空音轨（图片转 MIDI 自动建轨用），返回新音轨 id
    ///
    /// 同步维护全部音轨相关字段：notes / track_names / track_ports /
    /// track_max_end_ticks / tracks / track_count。
    pub fn add_empty_track(&mut self) -> u16 {
        let new_id = self.track_count;
        self.notes.push(crate::chunked_list::ChunkedList::new());
        self.track_names.push(None);
        self.track_ports.push(0);
        self.track_max_end_ticks
            .push(std::sync::Arc::new(std::sync::Mutex::new(None)));
        self.tracks.push(crate::track::TrackView::new(new_id));
        self.track_count = self.track_count.saturating_add(1);
        new_id
    }

    /// 在指定音轨按 start_tick 升序插入一个音符（保持每轨有序不变式）。
    /// 若 track_id 越界（音轨不存在）返回 false；成功返回 true。
    /// 同 start_tick 的音符插到已存在同 tick 音符之后（稳定插入）。
    pub fn insert_note(&mut self, track_id: usize, note: NoteEvent) -> bool {
        let Some(track_notes) = self.notes.get_mut(track_id) else {
            return false;
        };
        // 分块插入：只移动目标块内元素（O(块内)），满块自动分裂
        track_notes.insert(note);
        // 增量更新 max 缓存（脏时保持脏，查询时惰性重算）
        if let Some(cell) = self.track_max_end_ticks.get(track_id)
            && let Some(cur) = cell.lock().ok().and_then(|g| *g)
            && note.end_tick > cur
        {
            *cell.lock().unwrap_or_else(|e| e.into_inner()) = Some(note.end_tick);
        }
        true
    }

    /// 使指定音轨的 max_end_tick 缓存失效（置脏），下次查询时惰性重算。
    ///
    /// 毒锁时恢复（`into_inner`）而非 panic：缓存失效是保守操作，
    /// 即使锁被 panic 污染也不应中断编辑流程。
    fn invalidate_track_max_tick(&self, track_id: usize) {
        if let Some(cell) = self.track_max_end_ticks.get(track_id) {
            *cell.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    /// 删除指定音轨指定索引处的音符，返回被删除的音符副本。
    /// track_id 越界或 index 越界返回 None。
    pub fn remove_note(&mut self, track_id: usize, index: usize) -> Option<NoteEvent> {
        let removed = {
            let track_notes = self.notes.get_mut(track_id)?;
            track_notes.remove(index)
        };
        // 保守置脏：被删音符可能是当前 max，查询时惰性重算
        self.invalidate_track_max_tick(track_id);
        removed
    }

    /// 替换指定音轨指定索引处的音符：删除旧音符后按 start_tick 升序重新插入新音符，
    /// 保持每轨有序不变式。track_id 或 index 越界返回 false。
    pub fn update_note(&mut self, track_id: usize, index: usize, note: NoteEvent) -> bool {
        // 先删除旧音符；删除失败（track_id/index 越界）直接返回 false
        if self.remove_note(track_id, index).is_none() {
            return false;
        }
        // 删除成功已证明音轨存在，插入必然成功，不会出现中间不一致状态
        self.insert_note(track_id, note)
    }

    /// 返回指定音轨的可变音符引用（供批量编辑/排序场景使用）。
    /// track_id 越界返回 None。
    /// 注意：调用方必须保持 start_tick 升序不变式，本方法不校验。
    /// 返回后 max 缓存被置脏，下次 `track_max_end_tick` 查询时惰性重算。
    pub fn track_notes_mut(
        &mut self,
        track_id: usize,
    ) -> Option<&mut crate::chunked_list::ChunkedList<NoteEvent>> {
        // 可变引用逃逸后无法感知修改内容，保守置脏
        self.invalidate_track_max_tick(track_id);
        self.notes.get_mut(track_id)
    }

    /// 整轨替换音符（undo/redo 快照恢复专用）。
    ///
    /// `notes` 需按 start_tick 升序（调用方负责排序）；本方法直接整体赋值，
    /// 不做排序校验。track_id 越界返回 false。
    pub fn replace_track_notes(&mut self, track_id: usize, notes: Vec<NoteEvent>) -> bool {
        let Some(track) = self.notes.get_mut(track_id) else {
            return false;
        };
        *track = crate::chunked_list::ChunkedList::from_sorted(notes);
        self.invalidate_track_max_tick(track_id);
        true
    }

    /// 整轨替换音符（undo/redo 快照恢复专用，O(块数) 浅拷贝版）。
    ///
    /// 直接共享 `notes` 的块 Arc（`ChunkedList::clone` 为 O(块数) 指针拷贝），
    /// 不做数据复制——1600W 音符工程 undo/redo 恢复不再产生整轨拷贝。
    /// track_id 越界返回 false。
    pub fn replace_track_notes_chunked(
        &mut self,
        track_id: usize,
        notes: &crate::chunked_list::ChunkedList<NoteEvent>,
    ) -> bool {
        let Some(track) = self.notes.get_mut(track_id) else {
            return false;
        };
        *track = notes.clone();
        self.invalidate_track_max_tick(track_id);
        true
    }

    /// 清空指定音轨的所有音符。track_id 越界返回 false。
    pub fn clear_track_notes(&mut self, track_id: usize) -> bool {
        let Some(track) = self.notes.get_mut(track_id) else {
            return false;
        };
        track.clear();
        // 空轨缓存置脏（None），与 recompute 的空轨处理一致，避免残留 Some(0) 误判
        self.invalidate_track_max_tick(track_id);
        true
    }
}
