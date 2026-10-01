//! 画刷笔画确认/取消：覆盖格 × 粗细度 → 批量生成音符（√ / × 按钮）
//!
//! 与 `line_tool::confirm` 同构（`interaction/line_tool/confirm.rs`）：
//! - √：全部笔画（多条共存）一次性应用——覆盖集 → 层展开 → 按层解析音轨 →
//!   整体去重 → 写入 document（一次历史记录，Ctrl+Z 一次全退）；
//! - ×：全部丢弃（不清 document、不产生历史）。
//!
//! 性能：覆盖计算 O(覆盖格数)（与采样点数无关）；写入按规模二选一——
//! 小规模逐笔 `insert_note_with_id`（自动记录 GPU 段内增量事件），
//! 大规模走批量归并（O(N+M) 单次重建）并显式补标受影响轨。

use crate::{Editor, Note};
use lumino_editor_state::brush_tool::cov;
use lumino_note_core::history::CreateOp;
use std::collections::{BTreeMap, HashSet};

/// 批量归并写入阈值（音符数）：超过则走 `batch_insert_events_to_track_with_ids`
///
/// 逐笔插入的成本 ≈ 音符数 × 块内搬移 + 每次 `partition_point`；
/// 长笔画 × 大粗细度（如 5k 格 × 20 层）下逐笔插入会到百毫秒级，
/// 批量归并把它压成每轨一次 `extend_sorted`。阈值取自块大小量级，由 bench 标定。
const BATCH_INSERT_THRESHOLD: usize = 2048;

impl Editor {
    /// 确认全部笔画：按覆盖范围生成音符（按层写入分配音轨，一次历史记录）
    ///
    /// 返回是否实际生成了音符。成功后清空笔画与笔画历史。
    pub(crate) fn confirm_brush(&mut self) -> bool {
        let thickness = self.brush.thickness;
        if thickness == 0 || !self.editor_state.brush_tool.has_pending() {
            return false;
        }
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let strokes = self.editor_state.brush_tool.strokes.clone();

        // 1) 覆盖集 → 层展开 → (音轨, tick 格, key) 整体去重
        //
        // 去重口径：同音轨同 (tick, key) 只生成一个音符（多笔画重叠/相邻格层重叠
        // 不再像逐格盖戳那样产生重复音符）。
        let mut seen: HashSet<(usize, i64, u16)> = HashSet::new();
        let mut by_track: BTreeMap<usize, Vec<(f32, u16)>> = BTreeMap::new();
        let mut total = 0usize;
        for stroke in &strokes {
            let base_track = stroke.base_track;
            let cells = stroke.covered_cells(snap);
            let mut expanded = Vec::with_capacity(cells.len() * thickness as usize);
            cov::expand_layers(&cells, thickness, &mut expanded);
            for ((cell, key), level) in expanded {
                let track = self.brush_track_for_level(level as usize, base_track);
                if seen.insert((track, cell, key)) {
                    by_track
                        .entry(track)
                        .or_default()
                        .push((cov::cell_tick(cell, snap), key));
                    total += 1;
                }
            }
        }
        if total == 0 {
            return false;
        }

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
            for (&track, notes) in &by_track {
                if track >= track_count {
                    continue;
                }
                for &(tick, key) in notes {
                    let note = Note::new(tick, key, snap);
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
            }
        } else {
            // 大规模：按轨批量归并（须按 tick 升序；批量 API 只标当前轨，
            // 其余受影响轨由下面显式补标，否则洋葱皮增量漏刷新）
            for (&track, notes) in &by_track {
                if track >= track_count {
                    continue;
                }
                let mut events: Vec<lumino_midi_model::NoteEvent> = notes
                    .iter()
                    .map(|&(tick, key)| {
                        lumino_editor_state::note_to_event(Note::new(tick, key, snap))
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

        // 2) 一次历史记录（多轨合并为一组）+ 精确标记受影响轨
        self.editor_state.data.history.push_note_create(create_ops);
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        // 3) 清空笔画与笔画历史，驱动渲染刷新
        self.editor_state.brush_tool.reset();
        self.mark_notes_changed();
        true
    }

    /// 取消全部待确认笔画（× 按钮）
    pub(crate) fn cancel_brush(&mut self) {
        self.editor_state.brush_tool.clear_pending();
    }
}
