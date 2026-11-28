//! pending 批量复制的提交（异步）。

use crate::Editor;
use crate::note::Note;

impl Editor {
    /// 提交 pending 批量复制到 document（音符唯一权威）
    ///
    /// 在以下场景调用：
    /// - 复制拖动松手时（`handle_released` 的 `DraggingSelectionCopy` 分支，松手即提交）
    /// - `flush_pending_drag`（点击空白处，兜底）
    /// - `commit_current_edit()` 自动提交（Save/Play/Export 前的 fallback）
    ///
    /// 复制模式：将选中音符按 `pending_copy_drag_state.delta` 偏移后
    /// `batch_insert_notes` 写入内存层，并**只选中新插入的副本**
    /// （最新件框选；原件不再保留框选状态）。返回 `true` 表示已写入。
    /// 如果 pending 为 None 或 delta 为零，返回 false。
    pub fn commit_pending_copy(&mut self) -> bool {
        crate::puffin_profiler::commit_pending_copy();
        let Some(drag_state) = self.transient.pending_copy.as_ref() else {
            return false;
        };
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: pending 复制 delta 为零，跳过提交");
            self.transient.pending_copy = None;
            return false;
        }

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        // first-writer-wins 冲突判定：本地选择被更早的远端选择锁定时让行（远端优先）。
        if self.transient.local_selection_is_locked() {
            tracing::debug!("协作: 批量复制被远端抢先选择锁定，跳过（远端优先）");
            return false;
        }

        // 构造副本音符列表（原始位置 + delta，tick/key clamp 到合法范围）
        let notes: Vec<Note> = drag_state
            .selected_indices_fast()
            .into_iter()
            .filter_map(|i| self.editor_state.data.get_note_view(i))
            .map(|n| {
                let tick = (n.tick + drag_state.delta_tick as f32).max(0.0);
                let key =
                    (n.key as i32 + drag_state.delta_key as i32).clamp(0, max_key as i32) as u16;
                Note::from_raw(tick, key, n.length, n.velocity, n.channel)
            })
            .collect();
        if notes.is_empty() {
            self.transient.pending_copy = None;
            return false;
        }

        // 与粘贴提交一致：push history → 批量归并（单次重建，峰值仅单块 8MB，按值）
        self.push_history();
        let ids = self.editor_state.data.batch_insert_notes_with_ids(&notes);
        let inserted = ids.len();
        // 插入位移了既有音符索引，旧选中索引全部失效：清空后按参数全等
        // 重选「副本」（最新件框选；副本 tick 可能落在现有音符之间，索引散布
        // 而非连续追加，不能按 start..start+inserted 连续区间选中）。
        self.selection_clear();
        self.select_notes_by_params(&notes);
        self.mark_notes_changed();
        // 2026-09 去 ID 协作修复：复制拖拽（生成副本）属「增音符」，须广播给对端，
        // 否则 B 端完全缺失被复制的副本（按值批量广播，避免 100K 单消息风暴）。
        // 协作同步关闭时不构建载荷（消费端未连接会短路丢弃）。
        let track = self.editor_state.data.current_track;
        if !ids.is_empty() && self.editor_state.data.collab_sync_enabled() {
            let batch: Vec<(f32, u16, f32, u8, u8, usize)> = notes
                .iter()
                .map(|n| (n.tick, n.key, n.length, n.velocity, n.channel, track))
                .collect();
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_notes_added_batch(batch),
            ));
        }
        self.transient.pending_copy = None;
        // 编辑已提交：结束本地选择会话（通知对端）
        self.transient.emit_local_selection_changed(false, &[]);
        tracing::info!("Editor: 已复制 {} 个音符", inserted);
        true
    }
}
