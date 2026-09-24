//! 音符拖动流式应用与绘制完成（自 `notes.rs` 拆分，保持各文件 < 400 行）

use super::DragState;
use super::EditorData;
use super::Note;
use lumino_note_core::history::CreateOp;

impl EditorData {
    /// 流式应用拖动状态到当前音轨。
    ///
    /// 只修改 `drag_state` 选中的音符（直接改 document 当前轨）。
    /// 返回实际被修改的音符数。
    pub fn apply_drag_state_streaming(&mut self, drag_state: &DragState, max_key: u16) -> usize {
        if drag_state.is_delta_zero() {
            return 0;
        }

        let mut modified = 0usize;
        // 记录实际被修改的索引（主音轨增量事件：等长 UpdateRange）
        let mut modified_indices: Vec<usize> = Vec::new();

        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(self.current_track))
        {
            for (note_idx, selected) in drag_state.selected.iter().enumerate() {
                if !selected || note_idx >= track.len() {
                    continue;
                }
                if let Some(note) = track.get_mut(note_idx) {
                    let mut note_f = super::accessors::event_to_note(note);
                    if drag_state.apply_to_note(&mut note_f, max_key) {
                        note.start_tick = super::accessors::f32_to_tick(note_f.tick);
                        // 移动不改变长度：end_tick 跟随 start_tick 平移。
                        // 旧实现 end_tick 仅取 max(start+1)，右移时长度被压缩。
                        let new_end = (note.end_tick as i64 + drag_state.delta_tick)
                            .max(note.start_tick as i64 + 1)
                            as u32;
                        note.end_tick = new_end;
                        note.key = note_f.key as u8;
                        modified += 1;
                        modified_indices.push(note_idx);
                    }
                }
            }
        }

        if modified > 0 {
            // 增量对账：记录事件（内部 mark 置 dirty 后清除）
            self.record_update_ranges_streamed(&modified_indices);
        }
        modified
    }

    /// 完成绘制新音符（纯业务逻辑），返回创建的 Note
    pub fn finish_drawing(
        &mut self,
        start_tick: f32,
        key: u16,
        current_tick: f32,
        snap_precision: f32,
        default_note_length: f32,
    ) -> Option<Note> {
        if self.current_track == 0 {
            tracing::debug!("编辑器: Conductor 轨道禁止放置音符");
            return None;
        }
        let (tick, length) = if current_tick > start_tick {
            (start_tick, current_tick - start_tick)
        } else if current_tick < start_tick {
            (current_tick, start_tick - current_tick)
        } else {
            (start_tick, default_note_length)
        };
        let length = length.max(snap_precision);
        let note = Note::new(tick, key, length);
        let inserted_id = self.insert_note_with_id(self.current_track, note.clone());
        if let Some(id) = inserted_id {
            // 增量、极简操作日志：每 op 记录单个音符（20 字节）替代整轨快照克隆。
            // 合并窗口语义不变（300ms 内连续放置合并为一条 CreateEntry），
            // 但 undo/redo 恢复是 O(op 数) 精确位置操作，与音符总量解耦——
            // 1600W 音符工程铅笔绘制不再触发整轨快照（原 `push_history_mergeable` 路径
            // 每条都 `..top.clone()` 复制整个 EditorSnapshot）。
            // 关键：CreateOp 记录分配后的真实 id，redo 原样重插（身份稳定）。
            let op = CreateOp {
                track_id: self.current_track as u32,
                note: super::accessors::note_to_event(note.clone()).with_id(id),
            };
            let merged = self.push_note_create(vec![op]);
            if merged {
                tracing::debug!("编辑器: 音符放置已合并到当前 NoteCreate 日志");
            }
        }
        self.mark_current_track_changed();
        tracing::debug!("编辑器: 已保存 1 个音符到音轨 {}", self.current_track);
        Some(match inserted_id {
            Some(id) => note.with_id(id),
            None => note,
        })
    }
}
