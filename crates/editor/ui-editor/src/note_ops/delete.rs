//! 音符删除操作：delete_note_by_index / delete_note_at / delete_selected_notes
//!
//! `delete_selected_notes` 单次 O(N) 遍历 document 当前轨权威 `NoteEvent`
//! 捕获待删除音符信息（含 id），替代逐个 `get(i)` 的 O(K·log N) 开销。
//! 所有音符数据均以 `MidiDocument` 为唯一权威源（2026-08 单一权威源改造），
//! 不再经 `NoteView` 等派生视图承载身份字段。

use iced_core::Point;

use super::Editor;

impl Editor {
    /// 按索引删除单个音符，并清除悬停状态、标记数据变更、广播删除事件。
    ///
    /// # 参数
    /// * `index` — 待删除的音符索引
    pub fn delete_note_by_index(&mut self, index: usize) {
        // Capture note info before deletion for sync event
        // 2026-08 单一权威源：id 与字段取自 document 当前轨权威 NoteEvent，而非派生视图 NoteView
        let note_info = {
            let current_track = self.editor_state.data.current_track;
            let notes = self.editor_state.data.track_notes(current_track);
            notes.get(index).map(|n| {
                (
                    n.id,
                    n.start_tick as f32,
                    n.key as u16,
                    (n.end_tick - n.start_tick) as f32,
                    n.velocity,
                    n.channel,
                    current_track,
                )
            })
        };

        self.editor_state.data.delete_note_by_index(index);
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();

        // Emit sync event for deletion
        if let Some((id, tick, key, length, velocity, channel, track_idx)) = note_info {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_deleted(
                    id, tick, key, length, velocity, channel, track_idx,
                ),
            ));
        }
    }

    /// 删除指定屏幕坐标处命中的音符。
    ///
    /// # 参数
    /// * `pos` — 屏幕坐标
    ///
    /// # 返回
    /// 命中并删除音符返回 `true`，否则返回 `false`。
    pub fn delete_note_at(&mut self, pos: Point) -> bool {
        if let Some((index, _)) = self.hit_test_note(pos) {
            self.delete_note_by_index(index);
            true
        } else {
            false
        }
    }

    /// 删除所有选中的音符，并广播逐个音符的删除事件。
    pub fn delete_selected_notes(&mut self) {
        if !self.has_selection() {
            return;
        }

        // first-writer-wins 冲突判定：本地选择被更早的远端选择锁定时让行（远端优先），
        // 既不应用删除也不广播，避免覆盖远端已先提交的编辑。
        if self.local_selection_is_locked() {
            tracing::debug!("协作: 本地删除被远端抢先选择锁定，跳过（远端优先）");
            return;
        }

        // 协作同步关闭（未连接会话的默认态）时，逐音符 `LocalNoteDeleted` 广播
        // 的消费端会短路丢弃——跳过 O(N) 捕获与 K 次 emit（与粘贴/拖动/提交
        // 路径的 `collab_sync_enabled` 门控一致）。200W 选中时省 ~100MB 捕获
        // 缓冲 + 234MB 事件队列 + 逐条 emit/drain 开销。
        let collab_sync = self.editor_state.data.collab_sync_enabled();

        // 单次 O(N) 遍历捕获待删除音符信息，替代逐个 get(i) O(K·log N)
        // 2026-08 单一权威源：id 与字段取自 document 当前轨权威 NoteEvent，而非派生视图 NoteView
        let current_track = self.editor_state.data.current_track;
        let deleted_notes: Vec<_> = if collab_sync {
            let selected = &self.editor_state.interaction.selected_notes;
            let notes = self.editor_state.data.track_notes(current_track);
            notes
                .iter()
                .enumerate()
                .filter(|(i, _)| selected.contains(i))
                .map(|(_, n)| {
                    (
                        n.id,
                        n.start_tick as f32,
                        n.key as u16,
                        (n.end_tick - n.start_tick) as f32,
                        n.velocity,
                        n.channel,
                        current_track,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

        // 直接消费现有选中位图（免 `get_selected_indices` O(K) Vec + 位图重建往返；
        // 整轨全选时省 153MB 索引 Vec 与翻倍增长峰值）
        self.editor_state
            .data
            .delete_selected_notes(&self.editor_state.interaction.selected_notes);
        self.selection_clear();
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();

        // 编辑已提交：结束本地选择会话（通知对端）
        self.emit_local_selection_changed(false);

        // Emit sync events for each deleted note（仅协作会话开启时）
        for (id, tick, key, length, velocity, channel, track_idx) in deleted_notes {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_deleted(
                    id, tick, key, length, velocity, channel, track_idx,
                ),
            ));
        }
    }
}
