//! 异步提交 MoveOp 到后台线程
//!
//! 批量拖动（DraggingSelection）松手时，将实际数据更新放到后台线程，
//! UI 层每帧轮询 `poll_async_commit` 获取结果并推入历史记录。
//!
//! 2026-08 单一权威源改造：后台线程克隆当前音轨的 `Vec<NoteEvent>` 副本
//! （而非 im::Vector + track_notes 双份克隆），完成后经 `replace_track_notes` 写回。
//! 2026-09 身份统一：MoveOp 按全局唯一 id 定位（克隆副本内 tick 提示 + 全扫兜底），
//! 不依赖提交时的索引。

use super::EditorData;
use lumino_core::error::{CoreError, Result};
use lumino_midi_model::NoteEvent;
use lumino_note_core::history::MoveOp;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// 后台线程完成的异步提交结果
#[derive(Debug)]
pub struct AsyncCommitResult {
    /// 更新后的当前音轨音符（NoteEvent，与 document 轨道同构）
    pub notes: Vec<NoteEvent>,
    /// 实际修改的音符数
    pub modified: usize,
    /// 实际修改的连续索引区间（start, end_exclusive），供 GPU 段内增量更新
    pub modified_ranges: Vec<(usize, usize)>,
    /// 因就地修改 tick 发生重排时的**受影响索引区间集合**（升序、互不相交、
    /// 闭区间，最终索引空间）；非空时 `modified_ranges` 按旧索引已失效，
    /// 调用方改按本集合增量更新（区间外内容逐位不变，无全量重建）。
    pub reorder_ranges: lumino_midi_model::SortedRestoreRanges,
}

/// 待完成的异步提交
#[derive(Debug)]
pub struct PendingCommit {
    /// 待应用的操作日志
    pub ops: Vec<MoveOp>,
    /// 接收后台线程结果的通道
    pub receiver: Receiver<Result<AsyncCommitResult>>,
}

impl EditorData {
    /// 在后台线程异步应用 MoveOp 列表
    ///
    /// 返回 `true` 表示已启动新后台任务；`false` 表示 ops 为空或 delta 全为零。
    /// 同一时刻只允许一个 pending commit。
    pub fn apply_move_ops_async(&mut self, ops: Vec<MoveOp>, max_key: u16) -> Result<bool> {
        if ops.is_empty() || ops.iter().all(|op| op.delta_tick == 0 && op.delta_key == 0) {
            return Ok(false);
        }
        if self.pending_commit.is_some() {
            return Err(CoreError::InvalidArgument(
                "已存在 pending commit，无法启动新的异步提交".to_string(),
            ));
        }

        let notes = self.current_track_notes().to_vec();
        let ops_for_thread = ops.clone();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let result = apply_move_ops_to_clone(notes, &ops_for_thread, max_key);
            // 发送结果；如果接收端已关闭，忽略错误
            let _ = tx.send(result);
        });

        self.pending_commit = Some(PendingCommit { ops, receiver: rx });
        Ok(true)
    }

    /// 轮询异步提交是否完成
    ///
    /// 若完成：将结果应用到 data，把 MoveOp 推入历史记录，并返回实际修改数。
    /// 若未完成：返回 `None`。
    pub fn poll_async_commit(&mut self) -> Option<Result<usize>> {
        let pending = self.pending_commit.take()?;
        match pending.receiver.try_recv() {
            Ok(Ok(result)) => {
                let modified = result.modified;
                let reorder_ranges = result.reorder_ranges;
                // 先按增量区间构造 GPU 段内事件负载（此时 notes 仍可借用），
                // 再把整轨 move 写回 document——消除旧实现的整轨 clone。
                // 重排：旧索引区间失效 → 改按重排受影响区间集合（区间外内容不变）
                let mut update_events: Vec<(usize, Vec<lumino_note_core::note::Note>)> = Vec::new();
                if reorder_ranges.is_empty() {
                    update_events.reserve(result.modified_ranges.len());
                    for &(start, end) in &result.modified_ranges {
                        let notes: Vec<lumino_note_core::note::Note> = result.notes[start..end]
                            .iter()
                            .map(super::accessors::event_to_note)
                            .collect();
                        if !notes.is_empty() {
                            update_events.push((start, notes));
                        }
                    }
                } else {
                    for &(lo, hi) in &reorder_ranges {
                        let mut start = lo;
                        loop {
                            let end = (start + super::REORDER_SPAN_CHUNK).min(hi + 1);
                            let notes: Vec<lumino_note_core::note::Note> = result.notes[start..end]
                                .iter()
                                .map(super::accessors::event_to_note)
                                .collect();
                            if !notes.is_empty() {
                                update_events.push((start, notes));
                            }
                            if end > hi {
                                break;
                            }
                            start = end;
                        }
                    }
                }
                // 写回 document（当前音轨整轨替换，单一权威源）
                self.replace_track_notes(self.current_track, result.notes);
                for (start, notes) in update_events {
                    self.note_delta_events.push(
                        crate::editor_state::editor_data::NoteDeltaEvent::UpdateRange {
                            start_index: start,
                            notes,
                        },
                    );
                }
                // 异步提交作用于当前音轨，洋葱皮不显示 → 可豁免全量重建
                self.mark_current_track_changed();
                // 事件已完整记录（重排走受影响区间，未重排走实际修改区间）→ 清除 dirty
                self.note_delta_dirty = false;
                self.edited_tracks.insert(self.current_track);
                self.push_move_op(pending.ops);
                Some(Ok(modified))
            }
            Ok(Err(e)) => Some(Err(e)),
            Err(TryRecvError::Empty) => {
                self.pending_commit = Some(pending);
                None
            }
            Err(TryRecvError::Disconnected) => {
                Some(Err(CoreError::Other("异步提交线程异常断开".to_string())))
            }
        }
    }

    /// 是否有正在进行的异步提交
    pub fn has_pending_commit(&self) -> bool {
        self.pending_commit.is_some()
    }

    /// 取消正在进行的异步提交
    ///
    /// 仅用于重置或测试；正常流程依赖 `poll_async_commit`。
    pub fn cancel_async_commit(&mut self) {
        self.pending_commit = None;
    }
}

/// 将 MoveOp 应用到当前音轨音符的克隆副本（按全局唯一 id 定位）
fn apply_move_ops_to_clone(
    mut notes: Vec<NoteEvent>,
    ops: &[MoveOp],
    max_key: u16,
) -> Result<AsyncCommitResult> {
    let total_indices: usize = ops.iter().map(|op| op.ids.len()).sum();
    let start_time = std::time::Instant::now();
    tracing::info!(
        "异步提交线程启动: {} 个 op, 预计处理 {} 个音符",
        ops.len(),
        total_indices
    );

    let mut modified = 0usize;
    let mut modified_indices: Vec<usize> = Vec::new();
    let mut processed = 0usize;
    let mut next_log_threshold = total_indices / 10; // 每 10% 报告一次
    if next_log_threshold == 0 {
        next_log_threshold = total_indices; // 总数很小时只报告一次
    }

    for op in ops {
        let dt = op.delta_tick;
        let dk = op.delta_key as i32;
        let count = op
            .ids
            .len()
            .min(op.original_ticks.len())
            .min(op.original_keys.len());

        // 阶段 1：解析目标索引。提交路径恒为正向 op（undo/redo 走同步
        // `apply_move_ops`），克隆副本在提交窗口内冻结，tick 提示即原始位置。
        let mut resolved: Vec<usize> = Vec::with_capacity(count);
        for i in 0..count {
            let hint_tick = super::accessors::f32_to_tick(op.original_ticks[i].max(0.0));
            if let Some(idx) = position_of_id_in_slice(&notes, op.ids[i], hint_tick) {
                resolved.push(idx);
            }
        }

        // 阶段 2：原地应用（移动不改变长度：end_tick 跟随 start_tick 平移）。
        for idx in resolved {
            let note = &mut notes[idx];
            let new_tick = (note.start_tick as i64 + dt as i64).max(0) as u32;
            let new_key = (note.key as i32 + dk).clamp(0, max_key as i32) as u8;
            if note.start_tick != new_tick || note.key != new_key {
                note.start_tick = new_tick;
                let new_end = (note.end_tick as i64 + dt as i64).max(new_tick as i64 + 1) as u32;
                note.end_tick = new_end;
                note.key = new_key;
                modified += 1;
                modified_indices.push(idx);
            }
        }

        processed += count;
        if processed >= next_log_threshold && total_indices > 0 {
            let percent = processed
                .saturating_mul(100)
                .checked_div(total_indices)
                .unwrap_or(100);
            tracing::info!(
                "异步提交进度: {}% ({} / {})",
                percent,
                processed,
                total_indices
            );
            next_log_threshold += total_indices / 10;
        }
    }

    // 就地改 tick 破坏「按 start_tick 升序」不变式（window_range/position_of_id
    // 二分依赖，破坏后渲染/命中会漏检音符）→ 恢复；重排时旧索引区间失效。
    let (notes, reorder_ranges) =
        match lumino_midi_model::restore_sorted_vec(&notes, &modified_indices) {
            Some((restored, ranges)) => (restored, ranges),
            None => (notes, Vec::new()),
        };

    // 实际修改索引 → 连续区间（供 GPU 段内 UpdateRange 事件；仅未重排时有效）
    let modified_ranges = merge_consecutive_ranges(modified_indices);

    tracing::info!(
        "异步提交线程完成: 修改 {} 个音符, 重排区间数={}, 耗时 {:?}",
        modified,
        reorder_ranges.len(),
        start_time.elapsed()
    );

    Ok(AsyncCommitResult {
        notes,
        modified,
        modified_ranges,
        reorder_ranges,
    })
}

/// 在已按 tick 升序的音符切片中按 id 定位（tick 提示 + 全扫兜底）。
fn position_of_id_in_slice(notes: &[NoteEvent], id: u64, tick_hint: u32) -> Option<usize> {
    let start = notes.partition_point(|n| n.start_tick < tick_hint);
    let mut i = start;
    while i < notes.len() && notes[i].start_tick <= tick_hint {
        if notes[i].id == id {
            return Some(i);
        }
        i += 1;
    }
    notes.iter().position(|n| n.id == id)
}

/// 将（可重复、无序的）索引集合排序去重后合并为连续区间 `(start, end_exclusive)`。
///
/// 供异步提交结果构造 GPU 段内 `UpdateRange` 事件使用。
pub(super) fn merge_consecutive_ranges(mut indices: Vec<usize>) -> Vec<(usize, usize)> {
    indices.sort_unstable();
    indices.dedup();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    if let Some(&first) = indices.first() {
        let mut start = first;
        let mut prev = first;
        for &i in &indices[1..] {
            if i == prev + 1 {
                prev = i;
                continue;
            }
            ranges.push((start, prev + 1));
            start = i;
            prev = i;
        }
        ranges.push((start, prev + 1));
    }
    ranges
}

#[cfg(test)]
mod tests;
