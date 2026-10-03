//! 画刷笔画确认/取消：覆盖格 × 粗细度 → 批量生成音符（√ / × 按钮）
//!
//! 与 `line_tool::confirm` 同构（`interaction/line_tool/confirm.rs`）：
//! - √：全部笔画（多条共存）一次性应用——**待生成音符项与画布预览同源**
//!   （[`Editor::brush_pending_notes`]），一次历史记录，Ctrl+Z 一次全退；
//! - ×：全部丢弃（不清 document、不产生历史）、**笔画历史栈一并清空**。
//!
//! 性能：覆盖计算 O(覆盖格数)（与采样点数无关）；写入按规模二选一——
//! 小规模逐笔 `insert_note_with_id`（自动记录 GPU 段内增量事件），
//! 大规模走批量归并（O(N+M) 单次重建）并显式补标受影响轨。

use crate::{Editor, Note};
use lumino_note_core::history::CreateOp;
use std::collections::HashSet;

// 批量归并写入阈值：定义与完整理由集中在 `super::super::batch_insert`
// （曲线填充 / 形状 / 画刷三处共用，避免各写一份漂移）。
use super::super::BATCH_INSERT_THRESHOLD;

impl Editor {
    /// 确认全部笔画：按覆盖范围生成音符（按层写入分配音轨，一次历史记录）
    ///
    /// 返回是否实际生成了音符。成功后清空笔画与笔画历史。
    pub(crate) fn confirm_brush(&mut self) -> bool {
        let thickness = self.brush.thickness;
        if thickness == 0 {
            return false;
        }
        // 预览与写入的唯一权威源（所见即生成）
        let pending = self.brush_pending_notes();
        if pending.is_empty() {
            return false;
        }
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let total = pending.len();

        let track_count = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.track_count())
            .unwrap_or(0);
        let mut create_ops: Vec<CreateOp> = Vec::with_capacity(total);
        let mut affected: HashSet<usize> = HashSet::new();

        if total <= BATCH_INSERT_THRESHOLD {
            // 小规模：逐笔插入（当前轨自动记录 GPU 段内增量事件，各轨自行标脏）
            for item in &pending {
                let track = item.track;
                if track >= track_count {
                    continue;
                }
                let note = Note::new(item.tick_cell as f32 * snap, item.key, snap);
                if self
                    .editor_state
                    .data
                    .insert_note_with_id(track, note.clone())
                    .is_some()
                {
                    create_ops.push(CreateOp {
                        track_id: track as u32,
                        note: lumino_editor_state::note_to_event(note),
                    });
                    affected.insert(track);
                }
            }
        } else {
            // 大规模：按轨分组批量归并（须按 tick 升序；批量 API 只标当前轨，
            // 其余受影响轨由下面显式补标，否则洋葱皮增量漏刷新）
            let mut index = 0usize;
            while index < pending.len() {
                let track = pending[index].track;
                let mut end = index;
                while end < pending.len() && pending[end].track == track {
                    end += 1;
                }
                let group = &pending[index..end];
                index = end;
                if track >= track_count {
                    continue;
                }
                let mut events: Vec<lumino_midi_model::NoteEvent> = group
                    .iter()
                    .map(|item| {
                        lumino_editor_state::note_to_event(Note::new(
                            item.tick_cell as f32 * snap,
                            item.key,
                            snap,
                        ))
                    })
                    .collect();
                events.sort_by_key(|e| e.start_tick);
                self.editor_state
                    .data
                    .batch_insert_events_to_track_with_ids(track, events.clone());
                for note in events {
                    create_ops.push(CreateOp {
                        track_id: track as u32,
                        note,
                    });
                }
                affected.insert(track);
            }
        }

        if create_ops.is_empty() {
            return false;
        }

        // 一次历史记录（多轨合并为一组）+ 精确标记受影响轨
        self.editor_state.data.history.push_note_create(create_ops);
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        // 清空笔画与笔画历史（含撤销栈），驱动渲染刷新
        self.editor_state.brush_tool.reset();
        self.mark_notes_changed();
        true
    }

    /// 取消全部待确认笔画（× 按钮）：笔画与**笔画历史栈**一并清空
    pub(crate) fn cancel_brush(&mut self) {
        self.editor_state.brush_tool.clear_pending();
    }
}
