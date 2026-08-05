//! 异步提交 MoveOp 到后台线程
//!
//! 批量拖动（DraggingSelection）松手时，将实际数据更新放到后台线程，
//! UI 层每帧轮询 `poll_async_commit` 获取结果并推入历史记录。
//!
//! 2026-08 单一权威源改造：后台线程克隆当前音轨的 `Vec<NoteEvent>` 副本
//! （而非 im::Vector + track_notes 双份克隆），完成后经 `replace_track_notes` 写回。

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
                // 写回 document（当前音轨整轨替换，单一权威源）
                self.replace_track_notes(self.current_track, result.notes.clone());
                // 记录增量事件：MoveOp 区间 = notes 索引（等长），
                // 数据来自替换后的 result.notes（最终状态，重叠区间幂等）。
                for op in &pending.ops {
                    let start = op.range_start as usize;
                    let end = (op.range_end as usize).min(result.notes.len());
                    if start < end {
                        let notes: Vec<lumino_note_core::note::Note> = result.notes[start..end]
                            .iter()
                            .map(super::accessors::event_to_note)
                            .collect();
                        self.note_delta_events.push(
                            crate::editor_state::editor_data::NoteDeltaEvent::UpdateRange {
                                start_index: start,
                                notes,
                            },
                        );
                    }
                }
                // 异步提交作用于当前音轨，洋葱皮不显示 → 可豁免全量重建
                self.mark_current_track_changed();
                // 事件已完整记录（对应 ops 区间）→ 清除 dirty
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

/// 将 MoveOp 应用到当前音轨音符的克隆副本
fn apply_move_ops_to_clone(
    mut notes: Vec<NoteEvent>,
    ops: &[MoveOp],
    max_key: u16,
) -> Result<AsyncCommitResult> {
    let total_indices: usize = ops
        .iter()
        .map(|op| op.range_end.saturating_sub(op.range_start) as usize)
        .sum();
    let start_time = std::time::Instant::now();
    tracing::info!(
        "异步提交线程启动: {} 个 op, 预计处理 {} 个音符索引",
        ops.len(),
        total_indices
    );

    let mut modified = 0usize;
    let mut processed = 0usize;
    let mut next_log_threshold = total_indices / 10; // 每 10% 报告一次
    if next_log_threshold == 0 {
        next_log_threshold = total_indices; // 总数很小时只报告一次
    }

    for op in ops {
        let dt = op.delta_tick;
        let dk = op.delta_key as i32;

        let start = op.range_start as usize;
        let end = op.range_end as usize;
        for i in start..end {
            if let Some(note) = notes.get_mut(i) {
                let new_tick = (note.start_tick as i64 + dt as i64).max(0) as u32;
                let new_key = (note.key as i32 + dk).clamp(0, max_key as i32) as u8;
                if note.start_tick != new_tick || note.key != new_key {
                    note.start_tick = new_tick;
                    note.end_tick = note.end_tick.max(new_tick.saturating_add(1));
                    note.key = new_key;
                    modified += 1;
                }
            }

            processed += 1;
            if processed >= next_log_threshold {
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
    }

    tracing::info!(
        "异步提交线程完成: 修改 {} 个音符, 耗时 {:?}",
        modified,
        start_time.elapsed()
    );

    Ok(AsyncCommitResult { notes, modified })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_vec::BitVec;

    use crate::DragState;
    use lumino_note_core::note::Note;

    fn make_data_with_notes() -> EditorData {
        EditorData::with_f32_notes(
            1,
            &[
                Note::new(0.0, 60, 1.0),
                Note::new(10.0, 62, 1.0),
                Note::new(20.0, 64, 1.0),
            ],
        )
    }

    #[test]
    fn test_async_commit_applies_and_pushes_history() {
        let mut data = make_data_with_notes();
        let ops = data.move_ops_from_drag_state(&{
            let mut bv = BitVec::from_elem(3, false);
            bv.set(0, true);
            bv.set(2, true);
            let mut ds = DragState::new(bv, 0, 60);
            ds.set_delta(5, -2);
            ds
        });

        assert!(data.apply_move_ops_async(ops.clone(), 127).unwrap());
        let modified = loop {
            if let Some(result) = data.poll_async_commit() {
                break result.unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(modified, 2);
        assert_eq!(data.get_note_view(0).unwrap().tick, 5.0);
        assert_eq!(data.get_note_view(0).unwrap().key, 58);
        assert_eq!(data.get_note_view(2).unwrap().tick, 25.0);
        assert_eq!(data.get_note_view(2).unwrap().key, 62);
        assert!(data.history.can_undo());
        // undo 应还原
        assert!(data.undo());
        assert_eq!(data.get_note_view(0).unwrap().tick, 0.0);
        assert_eq!(data.get_note_view(0).unwrap().key, 60);
    }

    #[test]
    fn test_async_commit_zero_delta_is_noop() {
        let mut data = make_data_with_notes();
        let ops = vec![MoveOp {
            track_id: 1,
            range_start: 0,
            range_end: 2,
            delta_tick: 0,
            delta_key: 0,
            seq: 0,
            original_ticks: vec![],
            original_keys: vec![],
        }];
        assert!(!data.apply_move_ops_async(ops, 127).unwrap());
        assert!(!data.has_pending_commit());
    }

    #[test]
    fn test_async_commit_rejects_concurrent() {
        let mut data = make_data_with_notes();
        let ops1 = vec![MoveOp {
            track_id: 1,
            range_start: 0,
            range_end: 1,
            delta_tick: 1,
            delta_key: 0,
            seq: 0,
            original_ticks: vec![],
            original_keys: vec![],
        }];
        let ops2 = vec![MoveOp {
            track_id: 1,
            range_start: 1,
            range_end: 2,
            delta_tick: 1,
            delta_key: 0,
            seq: 0,
            original_ticks: vec![],
            original_keys: vec![],
        }];
        assert!(data.apply_move_ops_async(ops1, 127).unwrap());
        assert!(data.apply_move_ops_async(ops2, 127).is_err());
    }

    #[test]
    fn test_poll_async_commit_returns_none_while_pending() {
        let mut data = make_data_with_notes();
        let ops = vec![MoveOp {
            track_id: 1,
            range_start: 0,
            range_end: 1,
            delta_tick: 100,
            delta_key: 0,
            seq: 0,
            original_ticks: vec![],
            original_keys: vec![],
        }];
        assert!(data.apply_move_ops_async(ops, 127).unwrap());
        // 立即轮询可能返回 None（线程尚未完成）
        if data.poll_async_commit().is_none() {
            // 等待完成
            loop {
                if let Some(result) = data.poll_async_commit() {
                    result.unwrap();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        assert!(!data.has_pending_commit());
    }

    #[test]
    fn test_cancel_async_commit() {
        let mut data = make_data_with_notes();
        let ops = vec![MoveOp {
            track_id: 1,
            range_start: 0,
            range_end: 1,
            delta_tick: 10,
            delta_key: 0,
            seq: 0,
            original_ticks: vec![],
            original_keys: vec![],
        }];
        assert!(data.apply_move_ops_async(ops, 127).unwrap());
        data.cancel_async_commit();
        assert!(!data.has_pending_commit());
        // 数据不应被修改
        assert_eq!(data.get_note_view(0).unwrap().tick, 0.0);
    }
}
